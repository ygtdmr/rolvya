# Frontend third-party notices

Rolvya remains MIT licensed. The production JavaScript contains the complete notices for runtime dependencies as generated from the committed npm lockfile. `npm run licenses` verifies the runtime license inventory; build/test tooling is development-only.

- React and React DOM — MIT, Meta Platforms, Inc. and affiliates.
- React Flow (`@xyflow/react` and `@xyflow/system`) — MIT, webkid GmbH. The application retains the React Flow attribution.
- Other transitive runtime notices, including D3, Zustand, classcat and scheduler, are reproduced in the generated JS license banner.
- Inter, external `@fontsource/inter@5.3.0` — SIL Open Font License 1.1, Rasmus Andersson and contributors. [Source and license](https://github.com/rsms/inter/blob/master/LICENSE.txt).
- IBM Plex Mono, external `@fontsource/ibm-plex-mono@5.3.0` — SIL Open Font License 1.1, IBM Corp. [Source and license](https://github.com/IBM/plex/blob/master/LICENSE.txt).
- Lucide, external `lucide-static@1.66.0` SVG URLs — ISC; inherited Feather assets include their MIT notice. [Source and license](https://github.com/lucide-icons/lucide/blob/main/LICENSE).

Fonts and functional icons are fetched externally from version-pinned jsDelivr URLs. No font binary, icon font, SVG icon library or icon glyph payload is bundled. System fonts and visible text labels provide the offline/unavailable fallback. Network availability and actual rendering are separate from license permission.

The logo sheet is unchanged. No variant has been promoted to a canonical favicon or PWA icon without verified provenance.
