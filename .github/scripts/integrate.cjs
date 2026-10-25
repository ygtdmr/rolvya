'use strict';

// Runs only from the default branch. Never checkout, import, or execute PR code.
const policy = require('../integration-policy.json');

function authorIds(value) {
  const ids = JSON.parse(value || '[]');
  if (!Array.isArray(ids) || !ids.every(id => Number.isSafeInteger(id) && id > 0)) {
    throw new Error('Trusted author IDs must be a JSON array of positive integers');
  }
  return new Set(ids);
}

function routinePath(path) {
  if (typeof path !== 'string' || /[\\\x00-\x1f\x7f]/.test(path) ||
      path.split('/').some(part => part === '..' || part === '.' || part === '')) return false;
  // No application, policy, architecture, security, budget, infrastructure or
  // workflow paths are admitted. Expand this list only in an Owner-reviewed PR.
  return policy.routinePaths.some(pattern => pattern.endsWith('/**')
    ? path.startsWith(pattern.slice(0, -2)) && path.endsWith('.md')
    : path === pattern);
}

function candidate(pr, trusted) {
  return pr.state === 'open' && !pr.draft && !pr.locked &&
    pr.base?.ref === policy.baseBranch && pr.base?.repo?.id === policy.repositoryId &&
    pr.head?.repo?.id === policy.repositoryId && pr.head.ref.startsWith(policy.branchPrefix) &&
    trusted.has(pr.user?.id);
}

function routineFiles(pr, files) {
  return files.length > 0 && files.length === pr.changed_files && files.every(file =>
    ['added', 'modified', 'removed', 'renamed'].includes(file.status) &&
    routinePath(file.filename) && (!file.previous_filename || routinePath(file.previous_filename)));
}

function protectedRules(rules) {
  const status = rules.find(rule => rule.type === 'required_status_checks' &&
    rule.parameters?.strict_required_status_checks_policy === true &&
    rule.parameters.required_status_checks?.some(check =>
      check.context === policy.requiredCheck && check.integration_id === policy.actionsAppId));
  const review = rules.find(rule => rule.type === 'pull_request' &&
    rule.parameters?.require_code_owner_review === true &&
    rule.parameters.dismiss_stale_reviews_on_push === true &&
    rule.parameters.required_approving_review_count === 0 &&
    rule.parameters.require_last_push_approval === false);
  return Boolean(status && review && rules.some(rule => rule.type === 'deletion') &&
    rules.some(rule => rule.type === 'non_fast_forward'));
}

function ciEvidence(pr, workflow, runs, jobs, checks, statuses, trusted, required = []) {
  if (workflow.path !== policy.workflowPath || workflow.state !== 'active') return false;
  const run = [...runs].sort((a, b) => b.id - a.id)[0];
  if (!run || run.workflow_id !== workflow.id || run.event !== 'pull_request' ||
      run.head_sha !== pr.head.sha || run.head_repository?.id !== policy.repositoryId ||
      run.path !== policy.workflowPath || run.status !== 'completed' || run.conclusion !== 'success' ||
      !trusted.has(run.actor?.id) || !trusted.has(run.triggering_actor?.id) ||
      !run.pull_requests?.some(pull => pull.number === pr.number)) return false;
  if (!jobs.some(job => job.name === policy.requiredCheck && job.status === 'completed' &&
      job.conclusion === 'success' && job.run_id === run.id)) return false;
  // Reject failing or unfinished optional checks too. Required checks must be
  // successful (not neutral/skipped), and must come from GitHub Actions.
  if (!checks.some(check => check.name === policy.requiredCheck &&
      check.app?.id === policy.actionsAppId && check.status === 'completed' && check.conclusion === 'success')) return false;
  if (!checks.every(check => check.status === 'completed' &&
      ['success', 'neutral', 'skipped'].includes(check.conclusion))) return false;
  if (!statuses.every(status => status.state === 'success')) return false;
  if (!required.every(requirement => checks.some(check => check.name === requirement.context &&
      check.status === 'completed' && check.conclusion === 'success' &&
      (requirement.integration_id == null || check.app?.id === requirement.integration_id)) ||
      (requirement.integration_id == null && statuses.some(status =>
        status.context === requirement.context && status.state === 'success')))) return false;
  return true;
}

async function inspect({github, owner, repo, number, trusted}) {
  const args = {owner, repo};
  const {data: pr} = await github.rest.pulls.get({...args, pull_number: number});
  if (!candidate(pr, trusted)) return {eligible: false, reason: 'untrusted identity, fork, base, branch, draft or state'};
  const files = await github.paginate(github.rest.pulls.listFiles, {...args, pull_number: number, per_page: 100});
  if (!routineFiles(pr, files)) return {eligible: false, reason: 'sensitive/unfamiliar path, rename or incomplete file inventory'};
  if (pr.mergeable !== true || pr.mergeable_state !== 'clean') return {eligible: false, reason: 'conflict, pending checks or unknown mergeability'};
  const {data: settings} = await github.rest.repos.get(args);
  if (!settings.allow_squash_merge || settings.allow_merge_commit || settings.allow_rebase_merge || settings.allow_auto_merge) {
    return {eligible: false, reason: 'repository must allow squash only and disable native auto-merge enrollment'};
  }

  // GitHub's branch-rules endpoint requires Contents:read, not Administration.
  // Use a repository ruleset: classic protection alone does not satisfy this gate.
  const rules = await github.paginate('GET /repos/{owner}/{repo}/rules/branches/{branch}',
    {...args, branch: policy.baseBranch, per_page: 100});
  if (!protectedRules(rules)) return {eligible: false, reason: 'strict status/Code Owner branch rules not established'};
  for (const id of new Set(rules.map(rule => rule.ruleset_id))) {
    if (!Number.isSafeInteger(id)) return {eligible: false, reason: 'unverifiable ruleset identity'};
    const {data: ruleset} = await github.request('GET /repos/{owner}/{repo}/rulesets/{ruleset_id}', {...args, ruleset_id: id});
    if (ruleset.enforcement !== 'active' || !Array.isArray(ruleset.bypass_actors) || ruleset.bypass_actors.length) {
      return {eligible: false, reason: 'ruleset bypass or enforcement cannot be excluded'};
    }
  }
  const {data: branch} = await github.rest.repos.getBranch({...args, branch: policy.baseBranch});
  const {data: comparison} = await github.rest.repos.compareCommitsWithBasehead({...args, basehead: `${branch.commit.sha}...${pr.head.sha}`});
  if (!['ahead', 'identical'].includes(comparison.status) || comparison.behind_by !== 0) {
    return {eligible: false, reason: 'PR must include the current protected base'};
  }
  const {data: workflow} = await github.rest.actions.getWorkflow({...args, workflow_id: 'ci.yml'});
  const runs = await github.paginate(github.rest.actions.listWorkflowRuns, {...args, workflow_id: workflow.id,
    event: 'pull_request', head_sha: pr.head.sha, per_page: 100});
  const run = [...runs].sort((a, b) => b.id - a.id)[0];
  if (!run) return {eligible: false, reason: 'no CI run for this exact head'};
  const jobs = await github.paginate(github.rest.actions.listJobsForWorkflowRun,
    {...args, run_id: run.id, filter: 'latest', per_page: 100});
  const checks = await github.paginate(github.rest.checks.listForRef,
    {...args, ref: pr.head.sha, filter: 'latest', per_page: 100});
  const combined = await github.rest.repos.getCombinedStatusForRef({...args, ref: pr.head.sha, per_page: 100});
  // The combined endpoint caps statuses. Refuse incomplete evidence; do not let
  // a successful status hide another context outside the returned page.
  if (combined.data.total_count !== combined.data.statuses.length ||
      !ciEvidence(pr, workflow, runs, jobs, checks, combined.data.statuses, trusted,
        rules.flatMap(rule => rule.type === 'required_status_checks' ? rule.parameters.required_status_checks : []))) {
    return {eligible: false, reason: 'missing, stale, failing or untrusted check evidence'};
  }
  const {data: fresh} = await github.rest.pulls.get({...args, pull_number: number});
  if (!candidate(fresh, trusted) || fresh.head.sha !== pr.head.sha || fresh.base.sha !== pr.base.sha ||
      fresh.changed_files !== pr.changed_files || fresh.mergeable !== true || fresh.mergeable_state !== 'clean') {
    return {eligible: false, reason: 'PR changed during validation'};
  }
  return {eligible: true, reason: 'verified routine PR', sha: pr.head.sha};
}

async function run({github, context, core, mode, trustedAuthorIds}) {
  if (!['disabled', 'dry-run', 'enabled'].includes(mode || 'disabled')) throw new Error('Unknown merge mode');
  if (!mode || mode === 'disabled') {core.info('Integration is disabled'); return [];}
  if (`${context.repo.owner}/${context.repo.repo}` !== policy.repository ||
      context.payload.repository?.id !== policy.repositoryId ||
      context.ref !== `refs/heads/${policy.baseBranch}`) throw new Error('Wrong repository or untrusted workflow ref');
  const trusted = authorIds(trustedAuthorIds);
  if (!trusted.size) throw new Error('No trusted Codex author IDs configured');
  const {owner, repo} = context.repo;
  const pulls = await github.paginate(github.rest.pulls.list, {owner, repo, state: 'open', base: policy.baseBranch, per_page: 100});
  const results = [];
  for (const pull of pulls) {
    try {
      const result = await inspect({github, owner, repo, number: pull.number, trusted});
      results.push({number: pull.number, ...result});
      core.info(`#${pull.number}: ${result.reason}`);
      if (result.eligible && mode === 'enabled') {
        // No auto-merge enrollment: a later sensitive push must be reclassified.
        // Expected SHA plus GitHub's strict rules close head/base update races.
        const response = await github.rest.pulls.merge({owner, repo, pull_number: pull.number,
          merge_method: 'squash', sha: result.sha});
        if (!response.data.merged) throw new Error('GitHub did not confirm merge');
        core.info(`#${pull.number}: squash merged ${result.sha}`);
      }
    } catch (error) {
      core.setFailed(`#${pull.number}: validation/merge failed (${error.status || 'error'}); no bypass or blind retry`);
      results.push({number: pull.number, eligible: false, reason: 'API error or rejected merge'});
    }
  }
  await core.summary.addHeading(`Routine integration: ${mode}`).addCodeBlock(JSON.stringify(results, null, 2), 'json').write();
  return results;
}

module.exports = {authorIds, routinePath, candidate, routineFiles, protectedRules, ciEvidence, inspect, run};
