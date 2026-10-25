'use strict';
const fs = require('node:fs');
const path = require('node:path');
const {createHash} = require('node:crypto');

function safePath(value) {
  return typeof value === 'string' && /^[A-Za-z0-9_.\/-]+$/.test(value) &&
    !value.split('/').some(part => !part || part === '.' || part === '..');
}

const criticalPath = new RegExp(String.raw`(^|[/_.-])(` + [
  String.raw`auth\w*|oauth\w*|identity|login\w*|reauth\w*|passw\w*|principal\w*|nonce|apikey\w*`,
  String.raw`secur\w*|secret\w*|credential\w*|crypt\w*|key\w*|token\w*|ledger\w*|reserv\w*|admiss\w*`,
  String.raw`authorit\w*|payment\w*|balance\w*|quota\w*|budget\w*|bill\w*|financ\w*|cost\w*|approv\w*`,
  String.raw`permiss\w*|infra\w*|terraform|deploy\w*|backup\w*|recover\w*|execut\w*|orchestrat\w*|retry\w*`,
  String.raw`tls|ssh|cert\w*|migrat\w*|database|sql\w*|persist\w*|schedul\w*|worker\w*`,
  String.raw`publish\w*|github|git|audit\w*|session\w*|entitlement\w*|policy|rbac|csp`,
  String.raw`lease\w*|fenc\w*|outbox|dag|task\w*`,
].join('|') + String.raw`)([/_.-]|$)`);

function protectedPath(value) {
  if (!safePath(value)) return true;
  const normalized = value.replace(/([a-z])([A-Z])/g, '$1/$2').toLowerCase();
  return criticalPath.test(normalized) ||
    /^(\.github|\.codex|\.agents|docs\/(architecture|decisions|operations))\//i.test(value);
}

function sourceRule(pathname, configuration) {
  if (protectedPath(pathname) || pathname.split('/').some(part => part.startsWith('.'))) return undefined;
  return configuration.sourceRules.find(rule => (pathname.startsWith(rule.root + '/') || rule.tests.includes(pathname)) &&
    (rule.kind === 'rust' ? pathname.endsWith('.rs') : /\.(ts|tsx|css)$/.test(pathname)));
}

function routinePath(pathname, configuration) {
  if (!safePath(pathname) || protectedPath(pathname)) return false;
  return configuration.routinePaths.includes(pathname) || Boolean(sourceRule(pathname, configuration));
}

// Preserve array order, but canonicalize object keys. Only caller-specific and
// navigation fields are omitted; unknown configuration fields fail on mismatch.
// GitHub numeric ruleset IDs are immutable. node_id/created_at are read-only
// metadata, not part of the Owner-exported configuration. Preserve timestamp
// precision while accepting equivalent RFC3339 timezone representations.
function canonical(value) {
  if (Array.isArray(value)) return '[' + value.map(canonical).join(',') + ']';
  if (value && typeof value === 'object') return '{' + Object.keys(value).sort()
    .map(key => JSON.stringify(key) + ':' + canonical(value[key])).join(',') + '}';
  return JSON.stringify(value);
}

function rulesetDigest(ruleset) {
  const {bypass_actors, current_user_can_bypass, _links, node_id, created_at, ...configuration} = ruleset;
  configuration.updated_at = normalizeRevision(configuration.updated_at);
  return createHash('sha256').update(canonical(configuration)).digest('hex');
}

function validRevision(value) {
  return typeof value === 'string' && /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,3})?(?:Z|[+-]\d{2}:\d{2})$/.test(value) &&
    Number.isFinite(Date.parse(value));
}

function normalizeRevision(value) {
  if (!validRevision(value)) throw new Error('Missing or invalid ruleset revision timestamp');
  return new Date(value).toISOString();
}

function validateAttestations(configuration) {
  const attestations = configuration.rulesetAttestations;
  if (!Array.isArray(attestations)) throw new Error('Missing Owner ruleset revision attestations');
  const ids = new Set();
  for (const entry of attestations) {
    if (!entry || !Number.isSafeInteger(entry.rulesetId) || entry.rulesetId <= 0 || ids.has(entry.rulesetId) ||
        !validRevision(entry.updatedAt) ||
        !/^[0-9a-f]{64}$/.test(entry.configurationSha256 || '') || entry.verifiedBy !== configuration.account.id ||
        !Array.isArray(entry.bypassActors) || entry.bypassActors.length !== 0) {
      throw new Error('Invalid Owner ruleset attestation: exact identity/revision/digest and zero bypass actors required');
    }
    ids.add(entry.rulesetId);
  }
}

function validatePolicy(configuration, root) {
  const account = configuration.account;
  if (!account || !Array.isArray(configuration.sourceRules) || !Array.isArray(configuration.routinePaths)) {
    throw new Error('Missing single-account policy or paths');
  }
  if (configuration.humanOwners !== undefined || configuration.automationAuthors !== undefined ||
      !Number.isSafeInteger(account.id) || account.id <= 0 || account.id !== configuration.repositoryOwnerId ||
      !/^[a-z0-9][a-z0-9-]*$/i.test(account.login || '') || account.type !== 'User' ||
      account.id === configuration.integrationActorId || account.proofPr !== undefined) {
    throw new Error('Expected the pinned repository Owner User account; legacy independent-author roles/proofs are unsupported');
  }
  validateAttestations(configuration);
  for (const pathname of configuration.routinePaths) {
    if (!safePath(pathname) || protectedPath(pathname) || !pathname.endsWith('.md')) throw new Error('Unsafe routine documentation path');
  }
  const roots = [];
  for (const rule of configuration.sourceRules) {
    if (!safePath(rule.root) || protectedPath(rule.root) ||
        !['rust', 'frontend'].includes(rule.kind) ||
        (rule.kind === 'rust' ? !/^crates\/[\w-]+\/src\/[\w-]+(?:\/[\w-]+)*$/.test(rule.root)
          : !/^apps\/web\/src\/(ui|styles|i18n)\/[\w-]+(?:\/[\w-]+)*$/.test(rule.root)) ||
        !safePath(rule.manifest) || (rule.kind === 'rust' ? rule.manifest !== rule.root.split('/').slice(0, 2).join('/') + '/Cargo.toml' : rule.manifest !== 'apps/web/package.json') ||
        !Array.isArray(rule.tests) || !rule.tests.length || !rule.tests.every(test => safePath(test) && !protectedPath(test) &&
          (rule.kind === 'rust' ? test.startsWith(rule.root.split('/').slice(0, 2).join('/') + '/tests/') && test.endsWith('.rs') : /\.(test|spec)\.tsx?$/.test(test))) ||
        typeof rule.rationale !== 'string' || !rule.rationale.trim() ||
        roots.some(other => rule.root === other || rule.root.startsWith(other + '/') || other.startsWith(rule.root + '/'))) {
      throw new Error('Source rules need a narrow component root, manifest, tests and rationale');
    }
    roots.push(rule.root);
    if (!root) continue;
    const actual = relative => {
      let current = root;
      for (const part of relative.split('/')) {
        current = path.join(current, part);
        if (fs.lstatSync(current).isSymbolicLink()) throw new Error('Source registration cannot traverse symlinks');
      }
      return current;
    };
    if (!fs.statSync(actual(rule.root)).isDirectory() || !fs.statSync(actual(rule.manifest)).isFile() ||
        !rule.tests.every(test => fs.statSync(actual(test)).isFile())) throw new Error('Source boundary or test evidence does not exist');
    let count = 0, sources = 0;
    const walk = directory => {
      for (const entry of fs.readdirSync(actual(directory), {withFileTypes: true})) {
        const relative = directory + '/' + entry.name;
        if (++count > 2000 || entry.isSymbolicLink() || entry.name.startsWith('.') || protectedPath(relative)) throw new Error('Unsafe/overbroad source boundary');
        if (entry.isDirectory()) walk(relative);
        else if (sourceRule(relative, configuration)?.root === rule.root) sources++;
        else throw new Error('Non-source/configuration file inside routine source boundary');
      }
    };
    walk(rule.root);
    if (!sources) throw new Error('Empty speculative source boundary');
  }
  return true;
}

function codeowners(configuration) {
  return ['# Advisory ownership only: the sole Owner also authors Codex PRs.',
    '# Required CODEOWNER approval would deadlock; rules require zero reviews.',
    '# Sensitive changes stay excluded from the automatic merge controller.',
    '* @' + configuration.account.login, ''].join('\n');
}

function singleAccountReview(rule) {
  return rule.parameters?.require_code_owner_review === false &&
    rule.parameters.required_approving_review_count === 0 &&
    rule.parameters.require_last_push_approval === false &&
    rule.parameters.dismiss_stale_reviews_on_push === true &&
    rule.parameters.required_review_thread_resolution === true;
}

module.exports = {safePath, protectedPath, sourceRule, routinePath, validatePolicy, codeowners, singleAccountReview, canonical, rulesetDigest, validRevision, normalizeRevision};

if (require.main === module) {
  const root = path.resolve(process.argv[2] || path.join(__dirname, '../..'));
  const configuration = JSON.parse(fs.readFileSync(path.join(root, '.github/integration-policy.json'), 'utf8'));
  validatePolicy(configuration, root);
  if (fs.readFileSync(path.join(root, '.github/CODEOWNERS'), 'utf8') !== codeowners(configuration)) {
    throw new Error('CODEOWNERS and the reviewed integration policy differ');
  }
  const template = JSON.parse(fs.readFileSync(path.join(root, '.github/integration-ruleset.json'), 'utf8'));
  if (template.enforcement !== 'disabled' || template.bypass_actors?.length !== 0) {
    throw new Error('Ruleset template must stay disabled with no bypass actors until settings and protected smoke verification are complete');
  }
  const reviews = template.rules.filter(rule => rule.type === 'pull_request');
  if (!reviews.length || !reviews.every(singleAccountReview)) throw new Error('Single-account rules must not require self-approval');
  console.log('Single-account identity, source boundaries and advisory CODEOWNERS validated; protected smoke still required.');
}
