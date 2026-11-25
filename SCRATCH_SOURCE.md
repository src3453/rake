# Scratch source checkout

- Repository: [scratchfoundation/scratch-editor](https://github.com/scratchfoundation/scratch-editor)
- Commit: `40faacc808b02144767a8dd77db350b510c75b47`
- Retrieved: 2026-11-25
- Sparse checkout path: `packages/scratch-vm`
- Package version at this commit: `@scratch/scratch-vm` 15.3.0
- License: AGPL-3.0-only (see `work/scratch-editor/packages/scratch-vm/LICENSE`)

The checkout is at `work/scratch-editor`. The older standalone `scratch-vm` repository has been archived; this checkout uses the VM package in Scratch Foundation's current monorepo. The source is kept unchanged and is used to inspect the block/project format and validate generated projects.

The compiler itself does not import Scratch VM code. It writes Scratch's `project.json` block format and packages it with the project's SVG assets as an `.sb3` ZIP.
