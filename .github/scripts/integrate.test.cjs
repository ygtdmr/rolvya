'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {authorIds, routinePath, candidate, routineFiles, protectedRules, inspect, run} = require('./integrate.cjs');
const policy = require('../integration-policy.json');
const clone = value => structuredClone(value);

function fixture() {
  const state = {
    pr: {number: 7, state: 'open', draft: false, locked: false, user: {id: 72071644}, changed_files: 1,
      head: {ref: 'codex/harmless', sha: 'a'.repeat(40), repo: {id: policy.repositoryId}},
      base: {ref: 'main', sha: 'b'.repeat(40), repo: {id: policy.repositoryId}},
      mergeable: true, mergeable_state: 'clean', labels: [{name: 'owner-approved'}]},
    files: [{filename: 'docs/development/guides/routine-integration-smoke.md', status: 'added'}],
    workflow: {id: 99, path: policy.workflowPath, state: 'active'},
    runs: [{id: 55, workflow_id: 99, event: 'pull_request', head_sha: 'a'.repeat(40),
      head_repository: {id: policy.repositoryId}, path: policy.workflowPath, status: 'completed', conclusion: 'success',
      actor: {id: 72071644}, triggering_actor: {id: 72071644}, pull_requests: [{number: 7}]}],
    jobs: [{run_id: 55, name: 'Required checks', status: 'completed', conclusion: 'success'}],
    checks: [{name: 'Required checks', app: {id: policy.actionsAppId}, status: 'completed', conclusion: 'success'}],
    statuses: {total_count: 0, statuses: []},
    settings: {allow_squash_merge: true, allow_merge_commit: false, allow_rebase_merge: false, allow_auto_merge: false},
    rules: [
      {type: 'required_status_checks', ruleset_id: 1, parameters: {strict_required_status_checks_policy: true,
        required_status_checks: [{context: policy.requiredCheck, integration_id: policy.actionsAppId}]}},
      {type: 'pull_request', ruleset_id: 1, parameters: {require_code_owner_review: true,
        dismiss_stale_reviews_on_push: true, required_approving_review_count: 0, require_last_push_approval: false}},
      {type: 'deletion', ruleset_id: 1}, {type: 'non_fast_forward', ruleset_id: 1}],
    ruleset: {enforcement: 'active', bypass_actors: []},
    comparison: {status: 'ahead', behind_by: 0},
    branch: {commit: {sha: 'b'.repeat(40)}},
    calls: [], logs: [], errors: [], gets: 0,
  };
  function method(name, value) {
    return Object.assign(async args => {
      state.calls.push({name, args});
      if (state.failAt === name) throw Object.assign(new Error('simulated API failure'), {status: 403});
      return {data: clone(typeof value === 'function' ? value() : state[value])};
    }, {methodName: name});
  }
  const github = {rest: {
    pulls: {
      get: method('get', () => {state.gets++; return state.gets > 1 && state.fresh ? state.fresh : state.pr;}),
      list: method('pulls', () => [state.pr]), listFiles: method('files', 'files'),
      merge: method('merge', () => ({merged: state.merged !== false})),
    },
    repos: {get: method('settings', 'settings'), getBranch: method('branch', 'branch'), compareCommitsWithBasehead: method('comparison', 'comparison'),
      getCombinedStatusForRef: method('statuses', 'statuses')},
    actions: {getWorkflow: method('workflow', 'workflow'), listWorkflowRuns: method('runs', 'runs'),
      listJobsForWorkflowRun: method('jobs', 'jobs')},
    checks: {listForRef: method('checks', 'checks')},
  }, request: method('ruleset', 'ruleset')};
  github.paginate = async (endpoint, args) => typeof endpoint === 'string' ? clone(state.rules) : (await endpoint(args)).data;
  const core = {info: message => state.logs.push(message), setFailed: message => state.errors.push(message),
    summary: {addHeading() {return this;}, addCodeBlock() {return this;}, async write() {}}};
  const context = {repo: {owner: 'ygtdmr', repo: 'rolvya'}, ref: 'refs/heads/main', payload: {repository: {id: policy.repositoryId}}};
  return {state, github, core, context};
}

async function check(change = () => {}) {
  const fixtureData = fixture();
  change(fixtureData.state);
  const result = await inspect({github: fixtureData.github, owner: 'ygtdmr', repo: 'rolvya', number: 7,
    trusted: new Set([72071644])});
  return {result, ...fixtureData};
}

test('the normal routine case has complete evidence', async () => {
  const {result} = await check();
  assert.equal(result.eligible, true);
  assert.equal(result.sha, 'a'.repeat(40));
});

for (const path of [
  '.github/workflows/ci.yml', '.github/scripts/integrate.cjs', '.github/CODEOWNERS', 'AGENTS.md',
  '.codex/skills/graphify/SKILL.md', 'LICENSE', 'infra/main.tf', 'Dockerfile',
  'docs/architecture/security.md', 'docs/operations/secrets.md', 'docs/decisions/0002-hard-budget-approval.md',
  'apps/web/src/auth/session.ts', 'crates/budget/src/lib.rs', 'src/completely-new.rs',
  '../README.md', '/README.md', 'docs/../README.md', 'docs\\README.md', 'README.md\n::error::injection',
]) {
  test(`protected/unfamiliar path cannot be routine: ${JSON.stringify(path)}`, () => assert.equal(routinePath(path), false));
}

test('only reviewed paths are admitted; labels and self-approval cannot expand them', async () => {
  assert.equal(routinePath('README.md'), true);
  const {result} = await check(s => {s.files[0].filename = '.github/workflows/ci.yml';
    s.pr.labels = [{name: 'owner-approved'}, {name: 'automerge'}]; s.pr.self_approved = true;});
  assert.equal(result.eligible, false);
});

for (const [name, change] of [
  ['fork', s => {s.pr.head.repo.id = 123;}],
  ['wrong base', s => {s.pr.base.ref = 'unprotected';}],
  ['foreign repo', s => {s.pr.base.repo.id = 123;}],
  ['untrusted author', s => {s.pr.user.id = 999;}],
  ['branch spoof', s => {s.pr.head.ref = 'external/codex/harmless';}],
  ['draft', s => {s.pr.draft = true;}],
  ['closed', s => {s.pr.state = 'closed';}],
  ['locked', s => {s.pr.locked = true;}],
  ['conflict', s => {s.pr.mergeable = false;}],
  ['unknown mergeability', s => {s.pr.mergeable = null;}],
  ['dirty merge state', s => {s.pr.mergeable_state = 'dirty';}],
  ['incomplete file list', s => {s.pr.changed_files = 2;}],
  ['renamed protected file', s => {s.files[0].previous_filename = 'AGENTS.md'; s.files[0].status = 'renamed';}],
  ['deleted protected file', s => {s.files[0] = {filename: '.github/CODEOWNERS', status: 'removed'};}],
  ['unclassified file status', s => {s.files[0].status = 'changed';}],
  ['native auto-merge bypass route', s => {s.settings.allow_auto_merge = true;}],
  ['merge commits enabled', s => {s.settings.allow_merge_commit = true;}],
  ['squash disabled', s => {s.settings.allow_squash_merge = false;}],
  ['missing rules', s => {s.rules = [];}],
  ['non-strict checks', s => {s.rules[0].parameters.strict_required_status_checks_policy = false;}],
  ['spoofable check source', s => {s.rules[0].parameters.required_status_checks[0].integration_id = null;}],
  ['missing owner review rule', s => {s.rules[1].parameters.require_code_owner_review = false;}],
  ['stale approval allowed', s => {s.rules[1].parameters.dismiss_stale_reviews_on_push = false;}],
  ['bypass actor', s => {s.ruleset.bypass_actors = [{actor_type: 'RepositoryRole', actor_id: 5}];}],
  ['unobservable bypass list', s => {delete s.ruleset.bypass_actors;}],
  ['unenforced rules', s => {s.ruleset.enforcement = 'evaluate';}],
  ['stale base', s => {s.comparison = {status: 'diverged', behind_by: 1};}],
  ['missing CI', s => {s.runs = [];}],
  ['old successful CI', s => {s.runs[0].head_sha = 'c'.repeat(40);}],
  ['CI failure', s => {s.runs[0].conclusion = 'failure';}],
  ['CI cancellation', s => {s.runs[0].conclusion = 'cancelled';}],
  ['CI from other workflow', s => {s.runs[0].workflow_id = 100;}],
  ['CI dispatch spoof', s => {s.runs[0].event = 'workflow_dispatch';}],
  ['CI from fork', s => {s.runs[0].head_repository.id = 123;}],
  ['CI unrelated PR', s => {s.runs[0].pull_requests = [{number: 99}];}],
  ['untrusted rerun actor', s => {s.runs[0].triggering_actor.id = 999;}],
  ['newer CI in progress', s => {s.runs.push({...clone(s.runs[0]), id: 56, status: 'in_progress', conclusion: null});}],
  ['required job skipped', s => {s.jobs[0].conclusion = 'skipped';}],
  ['missing required check', s => {s.checks = [];}],
  ['check app spoof', s => {s.checks[0].app.id = 999;}],
  ['optional check fails', s => {s.checks.push({name: 'External', status: 'completed', conclusion: 'failure'});}],
  ['optional check pending', s => {s.checks.push({name: 'External', status: 'in_progress', conclusion: null});}],
  ['status fails', s => {s.statuses = {total_count: 1, statuses: [{context: 'External', state: 'failure'}]};}],
  ['status pagination incomplete', s => {s.statuses.total_count = 101;}],
  ['additional required check absent', s => {s.rules[0].parameters.required_status_checks.push({context: 'Extra', integration_id: null});}],
  ['head moves during validation', s => {s.fresh = clone(s.pr); s.fresh.head.sha = 'd'.repeat(40);}],
  ['base moves during validation', s => {s.fresh = clone(s.pr); s.fresh.base.sha = 'd'.repeat(40);}],
]) {
  test(`fails closed: ${name}`, async () => assert.equal((await check(change)).result.eligible, false));
}

test('trusted authors are explicit immutable IDs; malformed/empty trust never grants access', () => {
  assert.equal(authorIds('').size, 0);
  for (const value of ['["72071644"]', '[-1]', '{}', 'null', 'not-json']) assert.throws(() => authorIds(value));
  assert.equal(candidate(fixture().state.pr, new Set()), false);
});

test('disabled and dry-run modes never merge', async () => {
  for (const mode of ['disabled', 'dry-run']) {
    const f = fixture();
    await run({...f, mode, trustedAuthorIds: '[72071644]'});
    assert.equal(f.state.calls.some(call => call.name === 'merge'), false);
    assert.equal(f.state.errors.length, 0);
  }
});

test('enabled mode requests squash merge with an exact expected SHA', async () => {
  const f = fixture();
  await run({...f, mode: 'enabled', trustedAuthorIds: '[72071644]'});
  assert.deepEqual(f.state.calls.find(call => call.name === 'merge').args,
    {owner: 'ygtdmr', repo: 'rolvya', pull_number: 7, merge_method: 'squash', sha: 'a'.repeat(40)});
  assert.equal(f.state.errors.length, 0);
});

test('API denial and merge rejection never cause bypass, approval or automatic retry', async () => {
  for (const failAt of ['files', 'ruleset', 'jobs', 'merge']) {
    const f = fixture();
    f.state.failAt = failAt;
    await run({...f, mode: 'enabled', trustedAuthorIds: '[72071644]'});
    assert.equal(f.state.errors.length, 1);
    assert.ok(f.state.calls.filter(call => call.name === 'merge').length <= 1);
    if (failAt !== 'merge') assert.equal(f.state.calls.some(call => call.name === 'merge'), false);
  }
});

test('unknown mode, wrong repo, non-default ref and empty trust reject execution', async () => {
  const f = fixture();
  await assert.rejects(run({...f, mode: 'enable', trustedAuthorIds: '[72071644]'}));
  await assert.rejects(run({...f, mode: 'enabled', trustedAuthorIds: '[]'}));
  f.context.ref = 'refs/heads/codex/untrusted-workflow';
  await assert.rejects(run({...f, mode: 'enabled', trustedAuthorIds: '[72071644]'}));
  f.context.ref = 'refs/heads/main'; f.context.payload.repository.id = 123;
  await assert.rejects(run({...f, mode: 'enabled', trustedAuthorIds: '[72071644]'}));
});

test('CODEOWNERS protects all unknown files and exempts only the reviewed routine paths', () => {
  assert.equal(routineFiles({...fixture().state.pr, changed_files: 0}, []), false);
  assert.equal(protectedRules(fixture().state.rules), true);
});
