'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {authorIds, routinePath, candidate, routineFiles, protectedRules, inspect, run, verifyAccount, verifyActivation, securityPaths} = require('./integrate.cjs');
const policy = require('../integration-policy.json');
const clone = value => structuredClone(value);

function fixture() {
  const configuration = clone(policy);
  configuration.sourceRules = [];
  const state = {
    pr: {number: 7, state: 'open', draft: false, locked: false, user: {id: 72071644, login: 'ygtdmr', type: 'User'}, changed_files: 1,
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
    settings: {id: policy.repositoryId, owner: {id: 72071644, type: 'User'}, allow_squash_merge: true, allow_merge_commit: false, allow_rebase_merge: false, allow_auto_merge: false, delete_branch_on_merge: false},
    rules: [
      {type: 'required_status_checks', ruleset_id: 1, parameters: {strict_required_status_checks_policy: true,
        required_status_checks: [{context: policy.requiredCheck, integration_id: policy.actionsAppId}]}},
      {type: 'pull_request', ruleset_id: 1, parameters: {require_code_owner_review: false,
        dismiss_stale_reviews_on_push: true, required_approving_review_count: 0, require_last_push_approval: false, required_review_thread_resolution: true}},
      {type: 'required_linear_history', ruleset_id: 1}, {type: 'deletion', ruleset_id: 1}, {type: 'non_fast_forward', ruleset_id: 1}],
    ruleset: {enforcement: 'active', bypass_actors: []},
    comparison: {status: 'ahead', behind_by: 0},
    branch: {commit: {sha: 'b'.repeat(40)}},
    calls: [], logs: [], errors: [], gets: 0,
  };
  state.proofPr = {...clone(state.pr), number: 42, state: 'closed', merged: true, merged_at: '2026-10-25T10:01:00Z', merged_by: {id: policy.integrationActorId}, merge_commit_sha: 'd'.repeat(40)};
  state.controllerWorkflow = {id: 100, path: '.github/workflows/integrate.yml', state: 'active'};
  state.receipt = {id: 777, workflow_id: 100, path: '.github/workflows/integrate.yml', event: 'workflow_dispatch', head_branch: 'main', head_sha: 'b'.repeat(40), head_repository: {id: policy.repositoryId}, status: 'completed', conclusion: 'success', actor: {id: 72071644}, display_title: 'Protected smoke PR 42 at ' + 'a'.repeat(40), run_started_at: '2026-10-25T10:00:00Z', updated_at: '2026-10-25T10:02:00Z'};
  state.smokeJobs = [{run_id: 777, name: 'Protected smoke test', status: 'completed', conclusion: 'success'}];
  state.smokeFiles = clone(state.files);
  state.tree = {truncated: false, tree: securityPaths.map(path => ({path, mode: '100644', type: 'blob', sha: 'e'.repeat(40)}))};
  function method(name, value) {
    return Object.assign(async args => {
      state.calls.push({name, args});
      if (state.failAt === name) throw Object.assign(new Error('simulated API failure'), {status: 403});
      return {data: clone(typeof value === 'function' ? value(args) : state[value])};
    }, {methodName: name});
  }
  const github = {rest: {
    pulls: {
      get: method('get', args => {if (args.pull_number === 42) return state.proofPr; state.gets++; return state.gets > 1 && state.fresh ? state.fresh : state.pr;}),
      list: method('pulls', () => [state.pr]), listFiles: method('files', args => args.pull_number === 42 ? state.smokeFiles : state.files),
      merge: method('merge', () => ({merged: state.merged !== false})),
    },
    users: {getByUsername: method('identity', args => state.badIdentity || [configuration.account].find(identity => identity.login === args.username))},
    git: {getCommit: method('commit', args => ({tree: {sha: args.commit_sha}, parents: state.commitParents || [{sha: 'f'.repeat(40)}]})), getTree: method('tree', args => state.trees?.[args.tree_sha] || state.tree)},
    repos: {getCollaboratorPermissionLevel: method('permission', () => ({permission: state.humanPermission || 'admin'})), get: method('settings', 'settings'), getBranch: method('branch', 'branch'), compareCommitsWithBasehead: method('comparison', 'comparison'),
      getCombinedStatusForRef: method('statuses', 'statuses')},
    actions: {getWorkflow: method('workflow', args => args.workflow_id === 'integrate.yml' ? state.controllerWorkflow : state.workflow), getWorkflowRun: method('receipt', 'receipt'), listWorkflowRuns: method('runs', 'runs'),
      listJobsForWorkflowRun: method('jobs', args => args.run_id === 777 ? state.smokeJobs : state.jobs)},
    checks: {listForRef: method('checks', 'checks')},
  }, request: method('ruleset', 'ruleset')};
  github.paginate = async (endpoint, args) => typeof endpoint === 'string' ? clone(state.rules) : (await endpoint(args)).data;
  const core = {info: message => state.logs.push(message), setFailed: message => state.errors.push(message),
    summary: {addHeading() {return this;}, addCodeBlock() {return this;}, async write() {}}};
  const context = {repo: {owner: 'ygtdmr', repo: 'rolvya'}, ref: 'refs/heads/main', sha: 'b'.repeat(40), actor: 'ygtdmr', eventName: 'workflow_dispatch', payload: {repository: {id: policy.repositoryId}}};
  return {state, github, core, context, configuration, smokeRunId: '777', smokePr: '42'};
}

async function check(change = () => {}) {
  const fixtureData = fixture();
  change(fixtureData.state);
  const result = await inspect({github: fixtureData.github, owner: 'ygtdmr', repo: 'rolvya', number: 7,
    trusted: new Set([72071644]), configuration: fixtureData.configuration});
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
  ['automatic branch deletion enabled', s => {s.settings.delete_branch_on_merge = true;}],
  ['branch deletion setting unobservable', s => {delete s.settings.delete_branch_on_merge;}],
  ['merge commits enabled', s => {s.settings.allow_merge_commit = true;}],
  ['squash disabled', s => {s.settings.allow_squash_merge = false;}],
  ['missing rules', s => {s.rules = [];}],
  ['non-strict checks', s => {s.rules[0].parameters.strict_required_status_checks_policy = false;}],
  ['spoofable check source', s => {s.rules[0].parameters.required_status_checks[0].integration_id = null;}],
  ['CODEOWNER self-approval deadlock', s => {s.rules[1].parameters.require_code_owner_review = true;}],
  ['general approval deadlock', s => {s.rules[1].parameters.required_approving_review_count = 1;}],
  ['last-push approval deadlock', s => {s.rules[1].parameters.require_last_push_approval = true;}],
  ['unresolved review threads permitted', s => {s.rules[1].parameters.required_review_thread_resolution = false;}],
  ['missing linear history', s => {s.rules = s.rules.filter(rule => rule.type !== 'required_linear_history');}],
  ['missing delete protection', s => {s.rules = s.rules.filter(rule => rule.type !== 'deletion');}],
  ['missing force-push protection', s => {s.rules = s.rules.filter(rule => rule.type !== 'non_fast_forward');}],
  ['overlapping ruleset requires self-approval', s => {s.rules.push({type: 'pull_request', parameters: {require_code_owner_review: true, required_approving_review_count: 1}});}],
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

test('the shared Owner is the pinned author; external proof and legacy trust overrides are not required', async () => {
  assert.deepEqual([...authorIds()], [72071644]);
  const f = fixture();
  assert.equal(candidate(f.state.pr, authorIds(f.configuration), f.configuration), true);
  assert.equal(candidate(f.state.pr, new Set(), f.configuration), false);
  await assert.rejects(run({...f, mode: 'enabled', trustedAuthorIds: '[72071644]'}), /Legacy/);
  await verifyAccount(f.github, {owner: 'ygtdmr', repo: 'rolvya'}, f.configuration);
  assert.equal(f.state.calls.some(call => call.name === 'get'), false, 'no external identity proof PR is fetched');
});

test('disabled and dry-run modes never merge', async () => {
  for (const mode of ['disabled', 'dry-run']) {
    const f = fixture();
    await run({...f, mode});
    assert.equal(f.state.calls.some(call => call.name === 'merge'), false);
    assert.equal(f.state.errors.length, 0);
  }
});

test('enabled mode requests squash merge with an exact expected SHA', async () => {
  const f = fixture();
  await run({...f, mode: 'enabled'});
  assert.deepEqual(f.state.calls.find(call => call.name === 'merge').args,
    {owner: 'ygtdmr', repo: 'rolvya', pull_number: 7, merge_method: 'squash', sha: 'a'.repeat(40)});
  assert.equal(f.state.errors.length, 0);
});

test('API denial and merge rejection never cause bypass, approval or automatic retry', async () => {
  for (const failAt of ['files', 'ruleset', 'jobs', 'merge']) {
    const f = fixture();
    f.state.failAt = failAt;
    let rejected = false;
    try { await run({...f, mode: 'enabled'}); } catch { rejected = true; }
    assert.equal(f.state.errors.length + Number(rejected), 1);
    assert.ok(f.state.calls.filter(call => call.name === 'merge').length <= 1);
    if (failAt !== 'merge') assert.equal(f.state.calls.some(call => call.name === 'merge'), false);
  }
});

test('unknown mode, wrong repo and non-default ref reject execution', async () => {
  const f = fixture();
  await assert.rejects(run({...f, mode: 'enable'}));
  f.context.ref = 'refs/heads/codex/untrusted-workflow';
  await assert.rejects(run({...f, mode: 'enabled'}));
  f.context.ref = 'refs/heads/main'; f.context.payload.repository.id = 123;
  await assert.rejects(run({...f, mode: 'enabled'}));
});

test('advisory CODEOWNERS identifies the Owner without defining merge eligibility', () => {
  assert.equal(routineFiles({...fixture().state.pr, changed_files: 0}, []), false);
  assert.equal(protectedRules(fixture().state.rules), true);
  const advisory = codeowners(policy);
  assert.ok(advisory.includes('* @ygtdmr'));
  assert.equal(advisory.split('\n').filter(line => line && !line.startsWith('#')).length, 1);
});

test('the shipped ruleset preserves every required protection without self-approval or bypass', () => {
  const template = require('../integration-ruleset.json');
  assert.equal(template.enforcement, 'disabled');
  assert.deepEqual(template.bypass_actors, []);
  assert.deepEqual(template.conditions.ref_name, {include: ['refs/heads/main'], exclude: []});
  assert.equal(protectedRules(template.rules), true);
});

const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {validatePolicy, codeowners} = require('./integration-policy.cjs');

for (const [name, change] of [
  ['missing account', f => {delete f.configuration.account;}],
  ['wrong account ID', f => {f.configuration.account.id = 999;}],
  ['bot presented as Owner', f => {f.configuration.account.type = 'Bot';}],
  ['legacy independent roles', f => {f.configuration.automationAuthors = [];}],
  ['external identity proof gate', f => {f.configuration.account.proofPr = 7;}],
  ['renamed/reassigned login', f => {f.state.badIdentity = {id: 999, login: 'ygtdmr', type: 'User'};}],
  ['Owner has no write access', f => {f.state.humanPermission = 'read';}],
  ['actual repository owner differs', f => {f.state.settings.owner.id = 999;}],
]) {
  test(`account preflight blocks ${name}`, async () => {
    const f = fixture(); change(f);
    await assert.rejects(run({...f, mode: 'dry-run'}));
    assert.equal(f.state.calls.some(call => call.name === 'merge'), false);
  });
}

test('shared-account CI and reruns are accepted as verification, never independent approval', async () => {
  assert.equal((await check()).result.eligible, true);
  assert.equal((await check(s => {s.runs[0].actor.id = 999;})).result.eligible, false);
  assert.equal((await check(s => {s.pr.user.login = 'impostor';})).result.eligible, false);
  assert.equal((await check(s => {s.pr.user.type = 'Bot';})).result.eligible, false);
});

test('Owner-authored PR 7 is reusable as positive protected smoke without an approving review', async () => {
  const f = fixture();
  const result = await run({...f, mode: 'smoke-test', smokePr: 7, smokeHead: 'a'.repeat(40)});
  assert.equal(result[0].merged, true);
  assert.ok(f.state.calls.every(call => !/review|approv/i.test(call.name)));
});

test('isolated protected smoke merges one exact harmless shared-account PR with routine mode disabled', async () => {
  const f = fixture();
  const result = await run({...f, mode: 'smoke-test', routineMode: 'disabled', smokePr: 7, smokeHead: 'a'.repeat(40)});
  assert.equal(result.length, 1); assert.equal(result[0].merged, true);
  assert.equal(f.state.calls.filter(call => call.name === 'merge').length, 1);
});

for (const [name, change] of [
  ['missing exact head', f => {f.smokeHead = '';}],
  ['head mismatch', f => {f.smokeHead = 'c'.repeat(40);}],
  ['invalid selector', f => {f.smokePr = '7; arbitrary';}],
  ['automatic event', f => {f.context.eventName = 'workflow_run';}],
  ['unrecognized dispatch actor', f => {f.context.actor = 'rolvya-fixture[bot]';}],
  ['routine mode already enabled', f => {f.routineMode = 'enabled';}],
  ['protected change', f => {f.state.files[0].filename = '.github/workflows/ci.yml';}],
  ['rename into harmless name', f => {f.state.files[0].previous_filename = 'README.md'; f.state.files[0].status = 'renamed';}],
  ['failed CI', f => {f.state.runs[0].conclusion = 'failure';}],
  ['merge refused', f => {f.state.merged = false;}],
]) {
  test(`protected smoke refuses ${name}`, async () => {
    const f = {...fixture(), smokePr: 7, smokeHead: 'a'.repeat(40), routineMode: 'disabled'}; change(f);
    await assert.rejects(run({...f, mode: 'smoke-test'}));
  });
}

for (const [name, change] of [
  ['no receipt run ID', f => {f.smokeRunId = '';}],
  ['no receipt PR', f => {f.smokePr = '';}],
  ['unsuccessful workflow', f => {f.state.receipt.conclusion = 'failure';}],
  ['skipped smoke job', f => {f.state.smokeJobs[0].conclusion = 'skipped';}],
  ['dry-run job instead of actual smoke', f => {f.state.smokeJobs[0].name = 'Routine integration';}],
  ['non-manual receipt', f => {f.state.receipt.event = 'workflow_run';}],
  ['non-main receipt', f => {f.state.receipt.head_branch = 'codex/untrusted';}],
  ['wrong workflow', f => {f.state.receipt.workflow_id = 999;}],
  ['wrong repository', f => {f.state.receipt.head_repository.id = 123;}],
  ['foreign initiated receipt', f => {f.state.receipt.actor.id = 999;}],
  ['arbitrary run title', f => {f.state.receipt.display_title = 'approved';}],
  ['unmerged proof', f => {f.state.proofPr.merged = false;}],
  ['human manual merge instead of controller merge', f => {f.state.proofPr.merged_by.id = 72071644;}],
  ['foreign-authored smoke', f => {f.state.proofPr.user.id = 999;}],
  ['wrong smoke author login', f => {f.state.proofPr.user.login = 'impostor';}],
  ['wrong smoke author type', f => {f.state.proofPr.user.type = 'Bot';}],
  ['merge predates run', f => {f.state.proofPr.merged_at = '2026-10-24T10:00:00Z';}],
  ['merge follows run', f => {f.state.proofPr.merged_at = '2026-10-26T10:00:00Z';}],
  ['changed proof head', f => {f.state.proofPr.head.sha = 'c'.repeat(40);}],
  ['non-harmless proof files', f => {f.state.smokeFiles[0].filename = '.github/CODEOWNERS';}],
  ['renamed proof file', f => {f.state.smokeFiles[0].previous_filename = '.github/CODEOWNERS';}],
  ['non-squash commit', f => {f.state.commitParents = [{sha: 'a'}, {sha: 'b'}];}],
  ['truncated configuration tree', f => {f.state.tree.truncated = true;}],
  ['missing configuration file', f => {f.state.tree.tree.pop();}],
  ['security configuration changed after smoke', f => {
    f.state.receipt.head_sha = 'c'.repeat(40);
    const old = clone(f.state.tree); old.tree[0].sha = 'f'.repeat(40);
    f.state.trees = {['c'.repeat(40)]: old};
  }],
]) {
  test(`routine activation rejects ${name}`, async () => {
    const f = fixture(); change(f);
    await assert.rejects(run({...f, mode: 'enabled'}));
    assert.equal(f.state.calls.some(call => call.name === 'merge'), false);
  });
}

test('ordinary source changes do not invalidate the security snapshot', async () => {
  const f = fixture(); f.state.receipt.head_sha = 'c'.repeat(40);
  const old = clone(f.state.tree);
  old.tree.push({path: 'crates/example/src/format/value.rs', mode: '100644', type: 'blob', sha: '1'.repeat(40)});
  f.state.trees = {['c'.repeat(40)]: old};
  await verifyActivation(f);
});

function rule(kind) {
  return kind === 'rust'
    ? {kind, root: 'crates/text/src/format', manifest: 'crates/text/Cargo.toml', tests: ['crates/text/tests/format.rs'], rationale: 'Fixture: non-authoritative formatting only'}
    : {kind, root: 'apps/web/src/ui/primitives/spacing', manifest: 'apps/web/package.json', tests: ['apps/web/tests/spacing.test.ts'], rationale: 'Fixture: presentation-only spacing'};
}

for (const kind of ['rust', 'frontend']) {
  test(`registered ${kind} source and tests require existing boundaries and successful language CI`, async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'rolvya-source-policy-'));
    try {
      const f = fixture(), registration = rule(kind);
      f.configuration.sourceRules = [registration];
      const source = registration.root + (kind === 'rust' ? '/value.rs' : '/value.tsx');
      const write = relative => {const file = path.join(root, relative); fs.mkdirSync(path.dirname(file), {recursive: true}); fs.writeFileSync(file, 'fixture');};
      for (const relative of [registration.manifest, ...registration.tests, source]) write(relative);
      assert.equal(validatePolicy(f.configuration, root), true);
      assert.equal(routinePath(source, f.configuration), true);
      assert.equal(routinePath(registration.tests[0], f.configuration), true);
      assert.equal(codeowners(f.configuration), codeowners(policy), 'classification never manufactures a human review');
      f.state.files = [{filename: source, status: 'modified'}];
      const inspectFixture = () => inspect({...f, owner: 'ygtdmr', repo: 'rolvya', number: 7, trusted: new Set([72071644])});
      assert.equal((await inspectFixture()).eligible, false, 'a successful aggregate cannot hide skipped source CI');
      f.state.jobs.push({run_id: 55, name: kind === 'rust' ? 'Rust checks' : 'Frontend checks', status: 'completed', conclusion: 'success'});
      assert.equal((await inspectFixture()).eligible, true);
      for (const conclusion of ['failure', 'cancelled', 'skipped']) {
        f.state.jobs.at(-1).conclusion = conclusion;
        assert.equal((await inspectFixture()).eligible, false);
      }
      f.state.jobs.at(-1).conclusion = 'success';
      fs.symlinkSync(path.join(root, registration.manifest), path.join(root, registration.root, kind === 'rust' ? 'link.rs' : 'link.ts'));
      assert.throws(() => validatePolicy(f.configuration, root), /Unsafe/);
      fs.unlinkSync(path.join(root, registration.root, kind === 'rust' ? 'link.rs' : 'link.ts'));
      fs.unlinkSync(path.join(root, registration.tests[0]));
      assert.throws(() => validatePolicy(f.configuration, root));
    } finally {fs.rmSync(root, {recursive: true, force: true});}
  });
}

for (const relative of ['auth.rs', 'AuthSession.rs', 'Budget.rs', 'secret.rs', 'token.rs', 'policy.rs', 'migration.rs', 'audit.rs', '.env.rs', 'config.json', 'ledger.rs', 'reservation.rs', 'admission.rs', 'authority.rs', 'identity.rs', 'login.rs', 'password.rs', 'apikey.rs', 'recovery.rs', 'backup.rs']) {
  test(`critical/config file cannot use an approved source boundary: ${relative}`, () => {
    const f = fixture(); f.configuration.sourceRules = [rule('rust')];
    assert.equal(routinePath('crates/text/src/format/' + relative, f.configuration), false);
  });
}

test('future source paths are blocked until registered; broad/speculative/overlapping registrations fail', () => {
  const f = fixture();
  assert.equal(routinePath('crates/text/src/format/value.rs', f.configuration), false);
  for (const root of ['crates/text/src', 'crates', 'apps/web/src', 'apps/web/src/auth/session']) {
    f.configuration.sourceRules = [{...rule('rust'), root}];
    assert.throws(() => validatePolicy(f.configuration));
  }
  f.configuration.sourceRules = [rule('rust'), rule('rust')];
  assert.throws(() => validatePolicy(f.configuration));
  f.configuration.sourceRules = [rule('rust')];
  const empty = fs.mkdtempSync(path.join(os.tmpdir(), 'rolvya-no-source-'));
  try {assert.throws(() => validatePolicy(f.configuration, empty));} finally {fs.rmSync(empty, {recursive: true, force: true});}
});

test('sensitive shared-account PRs are excluded even with labels or fabricated approvals', async () => {
  const f = fixture();
  f.state.files[0].filename = '.github/integration-policy.json';
  f.state.pr.labels = [{name: 'owner-approved'}];
  f.state.pr.approved = true;
  const results = await run({...f, mode: 'enabled'});
  assert.equal(results[0].eligible, false);
  assert.equal(f.state.calls.some(call => call.name === 'merge'), false);
});

for (const failAt of ['identity', 'permission']) {
  test(`identity API permission denial fails closed: ${failAt}`, async () => {
    const f = fixture(); f.state.failAt = failAt;
    await assert.rejects(run({...f, mode: 'dry-run'}));
    assert.equal(f.state.calls.some(call => call.name === 'merge'), false);
  });
}

test('security symlinks cannot hide dependencies outside the verified tree', async () => {
  const f = fixture();
  f.state.tree.tree.push({path: '.github/scripts/hidden.py', mode: '120000', type: 'blob', sha: 'c'.repeat(40)});
  await assert.rejects(verifyActivation(f), /symlinks/);
});
