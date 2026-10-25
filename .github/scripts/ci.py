#!/usr/bin/env python3
"""Repository checks and manifest discovery; no application scaffolding."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[2]


def tracked_files():
    return [Path(os.fsdecode(p)) for p in subprocess.check_output(
        ['git', 'ls-files', '-z'], cwd=ROOT).split(b'\0') if p]


def inventory():
    files = tracked_files()
    manifests = [p for p in files if p.name == 'Cargo.toml']
    # Check workspace roots, avoiding duplicate checks of member crates.
    rust = [p for p in manifests if not any(q != p and q.parent in p.parents for q in manifests)]
    packages = [p for p in files if p.name == 'package.json']
    unexpected = [str(p) for p in packages if p != Path('apps/web/package.json')]
    if unexpected:
        raise ValueError(f'Unreviewed Node project locations: {unexpected}')
    if rust:
        for manifest in rust:
            if not (ROOT / manifest.parent / 'Cargo.lock').is_file():
                raise ValueError(f'Missing lockfile beside {manifest}')
        if not (ROOT / 'rust-toolchain.toml').is_file():
            raise ValueError('Rust implementation requires a pinned rust-toolchain.toml')
        import tomllib
        toolchain = tomllib.loads((ROOT / 'rust-toolchain.toml').read_text())['toolchain']['channel']
        if not re.fullmatch(r'1\.\d+\.\d+', toolchain):
            raise ValueError('Pin an exact stable Rust version before enabling Rust CI')
    else:
        toolchain = ''
    if packages:
        package = json.loads((ROOT / packages[0]).read_text())
        required = {'lint', 'typecheck', 'test:ci', 'build', 'verify:dist', 'check:browser-assets'}
        if not required.issubset(package.get('scripts', {})):
            raise ValueError(f'Frontend requires scripts {sorted(required)}')
        if not (ROOT / 'apps/web/package-lock.json').is_file():
            raise ValueError('Frontend requires a committed npm lockfile')
    migrations = any('migrations' in p.parts and p.suffix == '.sql' for p in files)
    if migrations and not (ROOT / '.github/scripts/check-migrations.sh').is_file():
        raise ValueError('SQL migrations require a reviewed .github/scripts/check-migrations.sh replay gate')
    if migrations and not rust:
        raise ValueError('SQL migrations require a Rust workspace to execute their replay gate')
    result = dict(rust=bool(rust), web=bool(packages), rust_manifests=[str(p) for p in rust],
                  rust_toolchain=toolchain, migrations=migrations)
    print(json.dumps(result))
    if os.environ.get('GITHUB_OUTPUT'):
        with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
            for key in ('rust', 'web', 'migrations'):
                output.write(f'{key}={str(result[key]).lower()}\n')
            output.write(f'rust_toolchain={toolchain}\n')
    return result


def repository_check():
    errors = []
    link_count = 0
    for relative in tracked_files():
        path = ROOT / relative
        if relative.suffix != '.md':
            continue
        text = path.read_text(encoding='utf-8')
        fence = None
        for number, line in enumerate(text.splitlines(), 1):
            marker = re.match(r'^\s*(`{3,}|~{3,})', line)
            if marker:
                if fence is None:
                    fence = marker.group(1)[0]
                elif marker.group(1)[0] == fence:
                    fence = None
                continue
            if fence:
                continue
            for href in re.findall(r'(?<!!)\[[^\]]+\]\((<[^>]+>|[^\s)]+)(?:\s+[^)]*)?\)', line):
                href = href.strip('<>')
                parts = urlsplit(href)
                if parts.scheme or parts.netloc or not parts.path:
                    continue
                target = (path.parent / unquote(parts.path)).resolve()
                if not target.is_relative_to(ROOT) or not target.exists():
                    errors.append(f'{relative}:{number}: missing relative link {href}')
                link_count += 1
        if fence:
            errors.append(f'{relative}: unclosed Markdown code fence')
    license_text = (ROOT / 'LICENSE').read_text()
    if not license_text.startswith('MIT License') or 'Permission is hereby granted' not in license_text:
        errors.append('MIT license notice is missing')
    if subprocess.check_output(['git', 'ls-files', 'graphify-out'], cwd=ROOT).strip():
        errors.append('Generated Graphify artifacts must not be tracked')
    result = subprocess.run(['node', str(ROOT / '.github/scripts/integration-policy.cjs'), str(ROOT)],
                            capture_output=True, text=True)
    if result.returncode:
        errors.append('CODEOWNERS/identity/source policy invalid: ' + result.stderr)
    policy = json.loads((ROOT / '.github/integration-policy.json').read_text())
    tracked = {str(path) for path in tracked_files()}
    for rule in policy.get('sourceRules', []):
        if rule['manifest'] not in tracked or not all(test in tracked for test in rule['tests']):
            errors.append('Registered source manifests and tests must be tracked Git files')
    base = os.environ.get('CI_BASE_SHA')
    head = os.environ.get('CI_HEAD_SHA')
    if base and head:
        if not all(re.fullmatch(r'[0-9a-f]{40}', sha) for sha in (base, head)):
            raise ValueError('Invalid commit IDs')
        subprocess.run(['git', 'diff', '--check', base, head], cwd=ROOT, check=True)
        changed = subprocess.check_output(['git', 'diff', '--name-only', '-z', base, head], cwd=ROOT).split(b'\0')
        names = {os.fsdecode(name) for name in changed if name}
        if any(name.endswith(('.rs', '.ts', '.tsx', '.js', '.cjs', '.mjs', '.py', '.css', '.sql')) for name in names):
            if 'docs/project/status.md' not in names:
                errors.append('Source/tooling changes require a project status update')
    if errors:
        raise ValueError('\n'.join(errors))
    print(f'PASS: repository documentation, MIT notice, CODEOWNERS and {link_count} relative links')


def verify_rust_registration(metadata):
    policy = json.loads((ROOT / '.github/integration-policy.json').read_text())
    packages = {str(Path(package['manifest_path']).resolve()): package for package in metadata['packages']}
    for rule in policy.get('sourceRules', []):
        if rule['kind'] != 'rust':
            continue
        package = packages.get(str((ROOT / rule['manifest']).resolve()))
        if not package or package['id'] not in metadata['workspace_members']:
            raise ValueError('Registered Rust component is not in the checked workspace')
        tests = {str(Path(target['src_path']).resolve()) for target in package['targets'] if 'test' in target['kind']}
        if not all(str((ROOT / test).resolve()) in tests for test in rule['tests']):
            raise ValueError('Registered Rust test evidence is not an executed Cargo test target')


def rust_check():
    data = inventory()
    if not data['rust']:
        raise ValueError('Rust job invoked without a Rust manifest')
    packages, members = [], []
    for manifest in data['rust_manifests']:
        metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--format-version', '1',
            '--no-deps', '--locked', '--manifest-path', manifest], cwd=ROOT, text=True))
        packages.extend(metadata['packages'])
        members.extend(metadata['workspace_members'])
    verify_rust_registration({'packages': packages, 'workspace_members': members})
    for manifest in data['rust_manifests']:
        commands = [
            ['cargo', 'fmt', '--manifest-path', manifest, '--all', '--', '--check'],
            ['cargo', 'clippy', '--manifest-path', manifest, '--workspace', '--all-targets', '--all-features', '--locked', '--', '-D', 'warnings'],
            ['cargo', 'test', '--manifest-path', manifest, '--workspace', '--all-features', '--locked'],
        ]
        for command in commands:
            subprocess.run(command, cwd=ROOT, check=True)
    if data['migrations']:
        subprocess.run(['bash', '.github/scripts/check-migrations.sh'], cwd=ROOT, check=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('command', choices=['inventory', 'repository', 'rust'])
    command = parser.parse_args().command
    {'inventory': inventory, 'repository': repository_check, 'rust': rust_check}[command]()
