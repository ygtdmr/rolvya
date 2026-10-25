# Security Policy

Thank you for helping keep Rolvya and its contributors safe. We welcome responsible, good-faith reports of security vulnerabilities in this repository and, when released, in Rolvya software maintained by this project.

## Supported versions and current status

Rolvya is in **pre-release development**. Its architecture and security requirements are documented, but the production orchestrator, remote worker runtime, API, UI, and Secret Broker have **not** been verified as implemented. See the [project status](docs/project/status.md).

| Version or distribution | Security maintenance |
| --- | --- |
| `main` (unreleased development) | Reports are reviewed on a best-effort basis; this is **not** a supported production release. |
| Official tagged releases | **None are currently published.** A version-specific support policy will be added when releases become available. |
| Third-party forks, modified builds, and historical snapshots | Not covered by a Rolvya maintenance commitment. |

No security certification, service-level agreement, or production-readiness guarantee is implied by this policy.

## Report a vulnerability privately

**Do not disclose a suspected exploitable vulnerability in a public GitHub issue, pull request, discussion, commit, or social-media post.**

The preferred reporting channel is the repository's [GitHub Security Advisories](https://github.com/ygtdmr/rolvya/security/advisories) page:

1. Choose **Report a vulnerability** if the button is available.
2. Submit a private report describing the impact and a minimal, safe reproduction.
3. Use that private advisory thread for follow-up and coordination.

This option works for external reporters **only when repository Private vulnerability reporting is enabled**. The maintainer must enable it in GitHub repository security settings. If the button is unavailable, **do not publish vulnerability details merely to reach the maintainer**. Use a private contact channel independently published by the maintainer, if one is available; otherwise retain the details privately until a suitable channel is established. No security-reporting email address or alternative confidential endpoint is currently designated in this file.

Please use ordinary [GitHub Issues](https://github.com/ygtdmr/rolvya/issues) only for non-sensitive bugs, feature requests, and documentation corrections that do not reveal an exploitable weakness.

## What to include

A useful private report contains:

- **Summary and impact:** what an attacker could gain or disrupt, the affected trust boundary, and prerequisites.
- **Affected target:** Rolvya component, exact commit SHA or release version, deployment configuration, and relevant environment.
- **Reproduction:** minimal, repeatable steps or a sanitized proof of concept, with expected versus observed behavior.
- **Evidence:** redacted logs, error messages, screenshots, or test fixtures sufficient to validate the claim.
- **Exposure:** whether the problem requires authentication, a particular permission, access to a worker, or control over a repository or model response.
- **Mitigation:** any safe workaround or proposed fix, if known.
- **Disclosure considerations:** whether details have already been shared with anyone else.

Do **not** send passwords, API tokens, private keys, live credentials, real user data, or unredacted customer artifacts. Describe their type and exposure instead. If a secret was inadvertently exposed, revoke or rotate it through the relevant provider promptly; do not paste it into a report or public history.

Reports may be written in English or Turkish.

## Security issues in scope

Please report demonstrable weaknesses affecting Rolvya-maintained code, configuration, or release artifacts, including:

- Authentication, authorization, session handling, account/project isolation, and privilege escalation.
- Secret handling, encryption, key lifecycle, token leakage, unsafe logging, and credential propagation.
- Untrusted prompts, model outputs, Git content, artifacts, or worker results crossing a trusted execution or approval boundary.
- Remote worker enrollment/isolation, command execution, lease and fencing validation, or malicious worker result acceptance.
- Immutable execution plans, dependency enforcement, authorization of billable work, hard budget approval gates, or accounting integrity.
- GitHub App permissions, protected Git publication, GitHub Actions workflows, dependency or build supply-chain compromise, and CI/merge-policy bypass.
- Unsafe file paths, URLs, deserialization, command construction, resource exhaustion, or unintended data disclosure.

These areas describe **potential reporting categories**, not assurances that the corresponding product features already exist. Approved security designs and implementation status are documented separately in [security architecture](docs/architecture/security.md), the [decision register](docs/decisions/README.md), and [project status](docs/project/status.md).

Security weaknesses in third-party services or infrastructure that Rolvya does not maintain should also be reported to their respective operators. Reports of a theoretical weakness in an unimplemented design are welcome as private security feedback, but should be identified as **design review**, not as evidence of an exploitable deployed vulnerability.

## Responsible research boundaries

Please:

- Test only repositories, installations, and accounts that you own or are explicitly authorized to assess.
- Prefer minimal proof of impact, isolated test fixtures, fake providers, and disposable environments.
- Stop after establishing sufficient evidence; avoid persistence, lateral movement, or accessing more data than necessary.
- Avoid denial-of-service testing, destructive changes, social engineering, phishing, unsolicited scanning, and activity against third-party systems without permission.
- Do not deliberately incur model API charges, provision paid cloud workers, extract credentials, or collect other users' data.
- Preserve confidentiality while a report is being assessed. Share enough detail privately for validation without increasing risk to others.

This policy does not grant additional access rights, authorize testing of GitHub, AWS, model-provider or third-party infrastructure, or establish a paid bug bounty or legal safe-harbor guarantee.

## Triage, remediation, and disclosure

Maintainers intend to:

1. **Triage privately:** validate the report, identify affected code or configuration, and assess practical impact, exploitability, and severity.
2. **Coordinate:** request clarification if needed, track a correction privately when appropriate, and avoid publishing exploit details before users can reasonably mitigate.
3. **Verify fixes:** use relevant tests, CI and regression evidence, and require the repository's established review/protected-merge process for security-sensitive changes.
4. **Disclose responsibly:** coordinate a public advisory, release notes, and a CVE/GHSA identifier where applicable, once disclosure is appropriate and a mitigation or fix is available.

We aim to acknowledge actionable reports in a reasonable time, but **do not currently promise a fixed response, remediation, or disclosure deadline**. Complex issues, pre-release status, and upstream dependencies may affect timelines. Reporters should coordinate publication timing through the private advisory thread rather than assume an automatic disclosure date.

Security-related fixes must not bypass protected branch requirements, approvals applicable to sensitive changes, or verification gates merely because they are urgent. A passing CI check alone does not establish a complete security review.

## Maintenance of this policy

This policy applies to the public repository at [ygtdmr/rolvya](https://github.com/ygtdmr/rolvya). Its reporting channel, supported release versions, and response expectations should be reviewed as the project gains maintainers and published releases. Changes to this file follow the repository's normal security-sensitive review process.
