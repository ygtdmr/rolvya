# Routine integration smoke test

This harmless documentation-only change exercises the repository's pull request CI. It changes no application behavior, credentials, security decisions or workflow configuration.

Before the CI bootstrap reaches main, this stacked PR verifies PR CI only. A live dry-run and protected automatic squash merge to main are still required by the [integration acceptance steps](../pull-request-integration.md#live-acceptance-and-activation).

Temporary negative CI fixture: [intentionally absent target](intentional-ci-negative-target.md).
