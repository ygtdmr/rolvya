'use strict';
const fs = require('node:fs');
const path = require('node:path');

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

function validatePolicy(configuration, root) {
  const humans = configuration.humanOwners;
  const authors = configuration.automationAuthors;
  if (!Array.isArray(humans) || !humans.length || !Array.isArray(authors) ||
      !Array.isArray(configuration.sourceRules) || !Array.isArray(configuration.routinePaths)) throw new Error('Missing policy roles/paths');
  const ids = new Set(), logins = new Set();
  for (const identity of [...humans, ...authors]) {
    if (!Number.isSafeInteger(identity.id) || identity.id <= 0 ||
        !/^[a-z0-9][a-z0-9-]*(?:\[bot\])?$/i.test(identity.login || '') ||
        !['User', 'Bot'].includes(identity.type) || ids.has(identity.id) || logins.has(identity.login.toLowerCase())) {
      throw new Error('Invalid, duplicate or overlapping human/automation identity');
    }
    ids.add(identity.id); logins.add(identity.login.toLowerCase());
  }
  if (humans.some(identity => identity.type !== 'User')) throw new Error('Human CODEOWNERS must be User identities');
  if (!humans.some(identity => identity.id === configuration.repositoryOwnerId)) throw new Error('Repository Owner must remain an independent human reviewer');
  if (authors.some(identity => identity.id === configuration.integrationActorId ||
      !Number.isSafeInteger(identity.proofPr) || identity.proofPr <= 0)) {
    throw new Error('Dedicated automation identity and a real independent-author proof PR are required');
  }
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
        else throw new Error('Non-source/configuration file inside ownerless source boundary');
      }
    };
    walk(rule.root);
    if (!sources) throw new Error('Empty speculative source boundary');
  }
  return true;
}

function codeowners(configuration) {
  const owners = configuration.humanOwners.map(identity => '@' + identity.login).join(' ');
  return ['# Sensitive and unfamiliar files require independent human review.', '* ' + owners, '',
    '# Owner-reviewed routine boundaries; generated from integration-policy.json.',
    ...configuration.routinePaths.map(pathname => '/' + pathname),
    ...configuration.sourceRules.flatMap(rule => ['/' + rule.root + '/**', ...rule.tests.map(test => '/' + test)]), ''].join('\n');
}

module.exports = {safePath, protectedPath, sourceRule, routinePath, validatePolicy, codeowners};

if (require.main === module) {
  const root = path.resolve(process.argv[2] || path.join(__dirname, '../..'));
  const configuration = JSON.parse(fs.readFileSync(path.join(root, '.github/integration-policy.json'), 'utf8'));
  validatePolicy(configuration, root);
  if (fs.readFileSync(path.join(root, '.github/CODEOWNERS'), 'utf8') !== codeowners(configuration)) {
    throw new Error('CODEOWNERS and the reviewed integration policy differ');
  }
  const template = JSON.parse(fs.readFileSync(path.join(root, '.github/integration-ruleset.json'), 'utf8'));
  if (template.enforcement !== 'disabled' || template.bypass_actors?.length !== 0) {
    throw new Error('Ruleset template must stay disabled with no bypass actors until identity preflight is complete');
  }
  console.log('Identity separation, source boundaries and CODEOWNERS validated; unconfigured authors do not activate merging.');
}
