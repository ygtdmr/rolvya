'use strict';

// Runs only from the default branch. Never checkout, import, or execute PR code.
const policy = require('../integration-policy.json');
const {routinePath: classifyPath, sourceRule, validatePolicy, singleAccountReview, canonical, rulesetDigest, validRevision, normalizeRevision} = require('./integration-policy.cjs');

function authorIds(configuration = policy) {
  validatePolicy(configuration);
  return new Set([configuration.account.id]);
}

function routinePath(path, configuration = policy) {
  return classifyPath(path, configuration);
}

function candidate(pr, trusted, configuration = policy) {
  return pr.state === 'open' && !pr.draft && !pr.locked &&
    pr.base?.ref === policy.baseBranch && pr.base?.repo?.id === policy.repositoryId &&
    pr.head?.repo?.id === policy.repositoryId && pr.head.ref.startsWith(policy.branchPrefix) &&
    trusted.has(pr.user?.id) && pr.user.id === configuration.account.id &&
    pr.user.login === configuration.account.login && pr.user.type === configuration.account.type;
}

function routineFiles(pr, files, configuration = policy) {
  return files.length > 0 && files.length === pr.changed_files && files.every(file =>
    ['added', 'modified', 'removed', 'renamed'].includes(file.status) &&
    routinePath(file.filename, configuration) && (!file.previous_filename || routinePath(file.previous_filename, configuration)));
}

async function verifyAccount(github, args, configuration) {
  const trusted = authorIds(configuration);
  const {data: repository} = await github.rest.repos.get(args);
  const account = configuration.account;
  if (repository.id !== configuration.repositoryId || repository.owner?.id !== account.id ||
      repository.owner?.type !== account.type) throw new Error('Repository ownership does not match the single-account policy');
  const {data: user} = await github.rest.users.getByUsername({username: account.login});
  if (user.id !== account.id || user.login !== account.login || user.type !== account.type) {
    throw new Error('GitHub account is missing, renamed or does not match its pinned identity');
  }
  const {data} = await github.rest.repos.getCollaboratorPermissionLevel({...args, username: account.login});
  if (!['admin', 'maintain', 'write'].includes(data.permission)) throw new Error('Owner account lacks repository write access');
  // This verifies account provenance only, never independent human authorization.
  return trusted;
}

function protectedRules(rules) {
  const status = rules.find(rule => rule.type === 'required_status_checks' &&
    rule.parameters?.strict_required_status_checks_policy === true &&
    rule.parameters.required_status_checks?.some(check =>
      check.context === policy.requiredCheck && check.integration_id === policy.actionsAppId));
  const reviews = rules.filter(rule => rule.type === 'pull_request');
  return Boolean(status && reviews.length && reviews.every(singleAccountReview) &&
    rules.some(rule => rule.type === 'required_linear_history') && rules.some(rule => rule.type === 'deletion') &&
    rules.some(rule => rule.type === 'non_fast_forward'));
}

async function protectionEvidence({github, args, configuration, report = () => {}}) {
  // Effective rules include active repository and inherited rulesets. Never
  // substitute the checked-in template or a PR-provided list for this endpoint.
  const rules = await github.paginate('GET /repos/{owner}/{repo}/rules/branches/{branch}',
    {...args, branch: configuration.baseBranch, per_page: 100});
  const reject = reason => ({rules, reason});
  if (!protectedRules(rules)) return reject('strict checks or deadlock-free single-account branch rules not established');
  const ids = new Set(rules.map(rule => rule.ruleset_id));
  if ([...ids].some(id => !Number.isSafeInteger(id) || id <= 0)) return reject('unverifiable ruleset identity');
  for (const id of ids) {
    const {data: ruleset} = await github.request('GET /repos/{owner}/{repo}/rulesets/{ruleset_id}', {...args, ruleset_id: id});
    const visible = Object.hasOwn(ruleset, 'bypass_actors');
    const digest = validRevision(ruleset.updated_at) ? rulesetDigest(ruleset) : undefined;
    // Public metadata only, useful for the Owner's browser-only attestation.
    // This is an observation, not an assertion that hidden bypass actors are empty.
    report('Ruleset observation: ' + JSON.stringify({rulesetId: ruleset.id,
      updatedAt: ruleset.updated_at, configurationSha256: digest, enforcement: ruleset.enforcement,
      currentUserCanBypass: ruleset.current_user_can_bypass, bypassActorsVisible: visible}));
    if (ruleset.id !== id || ruleset.enforcement !== 'active' || ruleset.target !== 'branch' ||
        ruleset.source_type !== 'Repository' || ruleset.source !== configuration.repository ||
        !validRevision(ruleset.updated_at) ||
        !Array.isArray(ruleset.rules) || !ruleset.conditions?.ref_name) {
      return reject(`ruleset ${id}: active identity, revision or configuration cannot be verified`);
    }
    if (ruleset.current_user_can_bypass !== 'never') return reject(`ruleset ${id}: current token bypass capability is not explicitly never`);
    if (visible && (!Array.isArray(ruleset.bypass_actors) || ruleset.bypass_actors.length !== 0)) {
      return reject(`ruleset ${id}: bypass actors are present or malformed`);
    }
    const entry = configuration.rulesetAttestations.find(entry => entry.rulesetId === id);
    if (!entry || normalizeRevision(entry.updatedAt) !== normalizeRevision(ruleset.updated_at) || entry.configurationSha256 !== digest) {
      return reject(`ruleset ${id}: missing or changed Owner-verified zero-bypass revision attestation`);
    }
    const effective = rules.filter(rule => rule.ruleset_id === id);
    const content = list => list.map(rule => canonical({type: rule.type, parameters: rule.parameters || {}})).sort();
    if (effective.some(rule => rule.ruleset_source_type !== ruleset.source_type || rule.ruleset_source !== ruleset.source) ||
        canonical(content(effective)) !== canonical(content(ruleset.rules))) {
      return reject(`ruleset ${id}: effective branch rules differ from the attested configuration`);
    }
  }
  if (ids.size !== configuration.rulesetAttestations.length) return reject('Owner-attested ruleset set no longer matches effective main rules');
  return {rules};
}

function ciEvidence(pr, workflow, runs, jobs, checks, statuses, trusted, required = [], files = [], configuration = policy) {
  if (workflow.path !== policy.workflowPath || workflow.state !== 'active') return false;
  const run = [...runs].sort((a, b) => b.id - a.id)[0];
  if (!run || run.workflow_id !== workflow.id || run.event !== 'pull_request' ||
      run.head_sha !== pr.head.sha || run.head_repository?.id !== policy.repositoryId ||
      run.path !== policy.workflowPath || run.status !== 'completed' || run.conclusion !== 'success' ||
      !trusted.has(run.actor?.id) ||
      !trusted.has(run.triggering_actor?.id) ||
      !run.pull_requests?.some(pull => pull.number === pr.number)) return false;
  if (!jobs.some(job => job.name === policy.requiredCheck && job.status === 'completed' &&
      job.conclusion === 'success' && job.run_id === run.id)) return false;
  const kinds = new Set(files.flatMap(file => [file.filename, file.previous_filename].filter(Boolean))
    .map(path => sourceRule(path, configuration)?.kind).filter(Boolean));
  for (const kind of kinds) {
    const name = kind === 'rust' ? 'Rust checks' : 'Frontend checks';
    if (!jobs.some(job => job.name === name && job.run_id === run.id &&
        job.status === 'completed' && job.conclusion === 'success')) return false;
  }
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

async function inspect({github, owner, repo, number, trusted, configuration = policy, report}) {
  validatePolicy(configuration);
  const args = {owner, repo};
  const {data: pr} = await github.rest.pulls.get({...args, pull_number: number});
  if (!candidate(pr, trusted, configuration)) return {eligible: false, reason: 'untrusted account, fork, base, branch, draft or state'};
  const files = await github.paginate(github.rest.pulls.listFiles, {...args, pull_number: number, per_page: 100});
  if (!routineFiles(pr, files, configuration)) return {eligible: false, reason: 'sensitive/unfamiliar path, rename or incomplete file inventory'};
  if (pr.mergeable !== true || pr.mergeable_state !== 'clean') return {eligible: false, reason: 'conflict, pending checks or unknown mergeability'};
  const {data: settings} = await github.rest.repos.get(args);
  if (!settings.allow_squash_merge || settings.allow_merge_commit || settings.allow_rebase_merge || settings.allow_auto_merge || settings.delete_branch_on_merge !== false) {
    return {eligible: false, reason: 'repository must allow squash only, disable native auto-merge and preserve head branches'};
  }

  const protection = await protectionEvidence({github, args, configuration, report});
  if (protection.reason) return {eligible: false, reason: protection.reason};
  const {rules} = protection;
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
        rules.flatMap(rule => rule.type === 'required_status_checks' ? rule.parameters.required_status_checks : []), files, configuration)) {
    return {eligible: false, reason: 'missing, stale, failing or untrusted check evidence'};
  }
  const {data: fresh} = await github.rest.pulls.get({...args, pull_number: number});
  if (!candidate(fresh, trusted, configuration) || fresh.head.sha !== pr.head.sha || fresh.base.sha !== pr.base.sha ||
      fresh.changed_files !== pr.changed_files || fresh.mergeable !== true || fresh.mergeable_state !== 'clean') {
    return {eligible: false, reason: 'PR changed during validation'};
  }
  // Recheck immediately before returning permission to merge: settings may have
  // changed while CI, checks and fresh PR state were being fetched.
  const finalProtection = await protectionEvidence({github, args, configuration});
  if (finalProtection.reason) return {eligible: false, reason: finalProtection.reason};
  return {eligible: true, reason: 'verified routine PR', sha: pr.head.sha};
}

const securityPaths = ['.github/scripts/integrate.cjs', '.github/scripts/integration-policy.cjs',
  '.github/integration-policy.json', '.github/CODEOWNERS', '.github/workflows/integrate.yml', '.github/workflows/ci.yml'];

function securitySnapshot(tree) {
  if (tree.truncated || !Array.isArray(tree.tree) ||
      !securityPaths.every(path => tree.tree.some(entry => entry.path === path && entry.type === 'blob' && entry.mode === '100644'))) {
    throw new Error('Incomplete or non-regular security configuration tree');
  }
  const entries = tree.tree.filter(entry => entry.path.startsWith('.github/') || entry.path === 'AGENTS.md');
  if (entries.some(entry => !((entry.type === 'blob' && ['100644', '100755'].includes(entry.mode)) ||
      (entry.type === 'tree' && entry.mode === '040000')))) throw new Error('Security configuration cannot contain symlinks or submodules');
  return JSON.stringify(entries.map(entry => [entry.path, entry.mode, entry.type, entry.sha]).sort((a, b) => a[0].localeCompare(b[0], 'en')));
}

async function verifyActivation({github, context, configuration, smokeRunId, smokePr}) {
  const positive = value => /^[1-9]\d*$/.test(String(value)) && Number.isSafeInteger(Number(value));
  if (!positive(smokeRunId) || !positive(smokePr)) throw new Error('Routine merging remains disabled until a protected smoke receipt exists');
  const args = {...context.repo};
  const {data: workflow} = await github.rest.actions.getWorkflow({...args, workflow_id: 'integrate.yml'});
  const {data: receipt} = await github.rest.actions.getWorkflowRun({...args, run_id: Number(smokeRunId)});
  const match = /^Protected smoke PR ([1-9]\d*) at ([0-9a-f]{40})$/.exec(receipt.display_title || '');
  if (!match || receipt.id !== Number(smokeRunId) || workflow.state !== 'active' ||
      Number(match[1]) !== Number(smokePr) || receipt.workflow_id !== workflow.id ||
      workflow.path !== '.github/workflows/integrate.yml' || receipt.path !== workflow.path ||
      receipt.event !== 'workflow_dispatch' || receipt.head_branch !== configuration.baseBranch ||
      receipt.head_repository?.id !== configuration.repositoryId || receipt.status !== 'completed' || receipt.conclusion !== 'success' ||
      receipt.actor?.id !== configuration.account.id) throw new Error('Untrusted or unsuccessful protected smoke run');
  const jobs = await github.paginate(github.rest.actions.listJobsForWorkflowRun,
    {...args, run_id: receipt.id, filter: 'latest', per_page: 100});
  if (!jobs.some(job => job.run_id === receipt.id && job.name === 'Protected smoke test' && job.status === 'completed' && job.conclusion === 'success')) {
    throw new Error('Receipt did not execute a successful protected smoke job');
  }
  const {data: proof} = await github.rest.pulls.get({...args, pull_number: Number(smokePr)});
  const merged = Date.parse(proof.merged_at), start = Date.parse(receipt.run_started_at), end = Date.parse(receipt.updated_at);
  if (!proof.merged || proof.head?.sha !== match[2] || proof.base?.ref !== configuration.baseBranch ||
      proof.base?.repo?.id !== configuration.repositoryId || proof.head?.repo?.id !== configuration.repositoryId ||
      proof.merged_by?.id !== configuration.integrationActorId ||
      (proof.user?.id !== configuration.account.id || proof.user?.login !== configuration.account.login || proof.user?.type !== configuration.account.type) ||
      !/^[0-9a-f]{40}$/.test(proof.merge_commit_sha || '') || !Number.isFinite(merged) || !(start <= merged && merged <= end)) {
    throw new Error('No pinned-account protected squash merge occurred in the recorded run');
  }
  const files = await github.paginate(github.rest.pulls.listFiles, {...args, pull_number: Number(smokePr), per_page: 100});
  if (proof.changed_files !== 1 || files.length !== 1 || files[0].filename !== configuration.smokePath ||
      files[0].previous_filename || !['added', 'modified'].includes(files[0].status)) throw new Error('Smoke receipt changed more than the harmless document');
  const {data: branch} = await github.rest.repos.getBranch({...args, branch: configuration.baseBranch});
  const {data: mergedCommit} = await github.rest.git.getCommit({...args, commit_sha: proof.merge_commit_sha});
  if (mergedCommit.parents?.length !== 1) throw new Error('Recorded smoke merge is not a single-parent squash commit');
  const {data: contained} = await github.rest.repos.compareCommitsWithBasehead({...args, basehead: `${proof.merge_commit_sha}...${branch.commit.sha}`});
  if (!['ahead', 'identical'].includes(contained.status) || contained.behind_by !== 0) throw new Error('Smoke merge is not on protected main');
  const snapshot = async sha => {
    const {data: commit} = await github.rest.git.getCommit({...args, commit_sha: sha});
    const {data: tree} = await github.rest.git.getTree({...args, tree_sha: commit.tree.sha, recursive: 'true'});
    return securitySnapshot(tree);
  };
  const verified = await snapshot(receipt.head_sha);
  for (const sha of new Set([context.sha, branch.commit.sha])) {
    if (await snapshot(sha) !== verified) throw new Error('Security configuration changed since protected smoke verification; repeat the smoke test');
  }
  return true;
}

async function run({github, context, core, mode, trustedAuthorIds, configuration = policy,
  smokeRunId, smokePr, smokeHead, routineMode}) {
  if (trustedAuthorIds !== undefined) throw new Error('Legacy unreviewed author overrides are unsupported');
  validatePolicy(configuration, require('node:path').join(__dirname, '../..'));
  if (!['disabled', 'dry-run', 'smoke-test', 'enabled'].includes(mode || 'disabled')) throw new Error('Unknown merge mode');
  if (!mode || mode === 'disabled') {core.info('Integration is disabled'); return [];}
  if (`${context.repo.owner}/${context.repo.repo}` !== policy.repository ||
      context.payload.repository?.id !== policy.repositoryId ||
      context.ref !== `refs/heads/${policy.baseBranch}`) throw new Error('Wrong repository or untrusted workflow ref');
  const {owner, repo} = context.repo;
  const trusted = await verifyAccount(github, {owner, repo}, configuration);
  if (mode === 'enabled') await verifyActivation({github, context, configuration, smokeRunId, smokePr});
  if (mode === 'smoke-test' && (context.eventName !== 'workflow_dispatch' || routineMode === 'enabled' ||
      configuration.account.login !== context.actor ||
      !/^[1-9]\d*$/.test(String(smokePr)) || !Number.isSafeInteger(Number(smokePr)) || !/^[0-9a-f]{40}$/.test(smokeHead || ''))) {
    throw new Error('Protected smoke test requires a main-branch manual dispatch, exact PR/head and inactive routine mode');
  }
  const pulls = mode === 'smoke-test' ? [{number: Number(smokePr)}]
    : await github.paginate(github.rest.pulls.list, {owner, repo, state: 'open', base: policy.baseBranch, per_page: 100});
  const results = [];
  for (const pull of pulls) {
    try {
      const result = await inspect({github, owner, repo, number: pull.number, trusted, configuration, report: message => core.info(message)});
      const record = {number: pull.number, ...result};
      results.push(record);
      core.info(`#${pull.number}: ${result.reason}`);
      if (result.eligible && mode === 'smoke-test') {
        const files = await github.paginate(github.rest.pulls.listFiles, {owner, repo, pull_number: pull.number, per_page: 100});
        if (result.sha !== smokeHead || files.length !== 1 || files[0].filename !== configuration.smokePath ||
            files[0].previous_filename || !['added', 'modified'].includes(files[0].status)) throw new Error('Smoke target is not the exact harmless revision');
      }
      if (result.eligible && ['enabled', 'smoke-test'].includes(mode)) {
        // No auto-merge enrollment: a later sensitive push must be reclassified.
        // Expected SHA plus GitHub's strict rules close head/base update races.
        const response = await github.rest.pulls.merge({owner, repo, pull_number: pull.number,
          merge_method: 'squash', sha: result.sha});
        if (!response.data.merged) throw new Error('GitHub did not confirm merge');
        record.merged = true;
        core.info(`#${pull.number}: squash merged ${result.sha}`);
      }
    } catch (error) {
      core.setFailed(`#${pull.number}: validation/merge failed (${error.status || 'error'}); no bypass or blind retry`);
      results.push({number: pull.number, eligible: false, reason: 'API error or rejected merge'});
    }
  }
  if (mode === 'smoke-test' && (results.length !== 1 || !results[0].merged)) throw new Error('No protected smoke merge verified; routine integration stays disabled');
  await core.summary.addHeading(`Routine integration: ${mode}`).addCodeBlock(JSON.stringify(results, null, 2), 'json').write();
  return results;
}

module.exports = {authorIds, routinePath, candidate, routineFiles, protectedRules, ciEvidence, inspect, run,
  verifyAccount, verifyActivation, securitySnapshot, securityPaths, protectionEvidence};
