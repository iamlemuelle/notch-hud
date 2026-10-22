# Vendored code

| Path | Source | Version | License |
|---|---|---|---|
| `vendor/mediaremote-adapter/` | https://github.com/ungive/mediaremote-adapter | tag `v0.7.7`, commit `e3ff5021eb0875858bd05f48d2e9ba2e962d1cf6` | BSD-3-Clause (`mediaremote-adapter/LICENSE`) |
| `dist/vendor/avatar-core.js` | npm `@bible-strong/avatar-core` (https://github.com/smontlouis/bible-strong-avatar-lab, `packages/avatar-core`) | `0.1.0`, tarball integrity `sha512-b5WAn7QiPlfjomKN9MvXm+Cirpff/yszZ7CJW+BfZSWnZSl+IUdG8zhaElimZAVcLNMzQ9WKngluhJlO6mNUCg==`; bundle sha256 `5094f122454a18830da8df387c5da1af5e8f8e65ce347860413a527f806e3c3d` | AGPL-3.0-only (the repository's `LICENSE`) |

mediaremote-adapter: copied without `.git` and `.vscode`, otherwise unmodified. `scripts/build-adapter.sh` builds it.
To update: clone the new tag, replace the directory, and change the row above.

avatar-core: `scripts/vendor-avatar-core.sh` downloads the tarball, checks the integrity above, and bundles it with
esbuild 0.28.2 into one classic script that defines `window.AvatarCore` (the page has no bundler). It is the
package's code unchanged, except that its one dependency, `ajv`, is replaced by a stub that throws if called: the
page never validates, Rust does (`src-tauri/src/avatar.rs`). The script is reproducible: a rerun prints the sha256
above. To update: change `VERSION` and `INTEGRITY` in the script, rerun it, and change the row above.

Both licenses fit this repository's AGPL-3.0: BSD-3-Clause code can be included in an AGPL work, and the renderer's
AGPL-3.0-only terms are why the repository uses the AGPL.
