# Contributing to Battle Frontier

Thanks for helping. Battle Frontier is an original game: every contribution must be your
own work, or work you have the right to submit under this project's licenses.

## From the editor to the game

The game's own editor makes most of what the game is: open the game and click **Editor** in
the bottom-right corner of its main menu. Maps, props, creatures' looks, abilities and their
effects are all made there.

- In the desktop app (`cargo run -p engine-game --release`), what you save is written into
  the project: `content/data/<collection>/<id>.json` and `content/assets/`.
- In the browser, saving downloads each file: put it in the same place in a clone of the
  repository.

Then commit those files on a branch of your fork and open a pull request (below). Once it's
reviewed and merged, everyone plays it.

## What a pull request can contain

- **Original work only.** No names, text, pictures, sounds, music, models or data taken
  from other games or anyone else's work without the right to share it. The game's
  content lives in `content/`; local placeholders (`temporary/`) and private tools
  (`private/`) are never committed.
- **Content** (creatures, moves, maps, props...): one record per file under
  `content/data/<collection>/<id>.json`, ids in lowercase (`a-z`, `0-9`, `_`), records
  referring to each other by id. Run `cargo run -p engine-content --bin content -- format`
  so the files keep the standard layout, and `-- check` so every reference resolves.
- **Models** (glTF, `.glb`): in `content/assets/models/`, named by the prop's `model` field.
  The Blender add-on in `tools/blender/` adds a model and its record for you, measured and
  laid out as the project's own; see `tools/blender/README.md`.
- **Code**: in `crates/`, matching the style around it.

Before opening a pull request:

```bash
cargo test --workspace
cargo clippy --workspace
cargo run -p engine-content --bin content -- check
trunk build --release
python3 tools/check-names.py
```

Every pull request is checked automatically (`.github/workflows/checks.yml`): the tests,
the lints (warnings fail), the content check, the site built as Cloudflare Pages builds
it, a scan for other games' brand names (kept as hashes in `tools/forbidden-names.sha256`,
so the names never appear here), and a sign-off on
every commit. Only the maintainers merge pull requests into `main`.

## Licenses

- The code (everything outside `content/`) is under the MIT license or the Apache
  License 2.0, at your choice (`LICENSE-MIT`, `LICENSE-APACHE`).
- The game's content (`content/`) is under Creative Commons Attribution-ShareAlike 4.0
  (`content/LICENSE`).

By contributing, you agree that your contribution is licensed the same way: code under
MIT or Apache-2.0, content under CC BY-SA 4.0.

## Sign your commits

Every commit must carry a sign-off line certifying the Developer Certificate of Origin
below (`git commit -s` adds it):

```
Signed-off-by: Your Name <your.email@example.com>
```

```
Developer Certificate of Origin
Version 1.1

Copyright (C) 2004, 2006 The Linux Foundation and its contributors.

Everyone is permitted to copy and distribute verbatim copies of this
license document, but changing it is not allowed.


Developer's Certificate of Origin 1.1

By making a contribution to this project, I certify that:

(a) The contribution was created in whole or in part by me and I
    have the right to submit it under the open source license
    indicated in the file; or

(b) The contribution is based upon previous work that, to the best
    of my knowledge, is covered under an appropriate open source
    license and I have the right under that license to submit that
    work with modifications, whether created in whole or in part
    by me, under the same open source license (unless I am
    permitted to submit under a different license), as indicated
    in the file; or

(c) The contribution was provided directly to me by some other
    person who certified (a), (b) or (c) and I have not modified
    it.

(d) I understand and agree that this project and the contribution
    are public and that a record of the contribution (including all
    personal information I submit with it, including my sign-off) is
    maintained indefinitely and may be redistributed consistent with
    this project or the open source license(s) involved.
```
