# Routine integration smoke test

This documentation-only file is used to verify Rolvya's protected pull-request CI and automated squash integration. It does not change application behavior, credentials, security policy, or workflow configuration.

## Verification after GitHub Actions dependency updates

This revision confirms that the upgraded GitHub Actions dependencies are exercised by a new pull-request CI run before a fresh protected smoke merge.

The Owner must keep `ROLVYA_MERGE_MODE=disabled` while conducting the isolated `smoke-test` workflow. After the protected squash merge succeeds, record that run and PR number in the verification variables, then enable routine integration according to the [integration runbook](../pull-request-integration.md).
