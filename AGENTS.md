# NODES development instructions

## Implementation language

The owner's decision, recorded on 2026-11-27 JST, is **Rust first, with a small
amount of other languages where needed**. Use [docs/LANGUAGE_POLICY.md](docs/LANGUAGE_POLICY.md)
as the common implementation-language policy for this repository.

- Implement the application, native UI, graph and Data Tree engine, standard
  nodes, geometry kernels, persistence, file I/O and CLI in Rust.
- Keep other languages limited to a specific supporting role. Existing Python
  developer utilities, PowerShell build helpers, CI commands and bounded Rhai
  user scripts are permitted. Keep the normal application usable without an
  external Python, Node.js or .NET runtime.
- Explain the need, Rust alternative, boundary and dependency impact of any new
  non-Rust implementation in the change. A small adapter is not authorization
  to move a whole core subsystem to another language.
- Preserve existing working functionality. Do not remove user scripting, rewrite
  useful tooling or alter saved projects simply to increase a language percentage.

## Project constraints

- Keep the desktop application independent of Rhino/Grasshopper, Windows first,
  lightweight and Japanese first. Continue the native Rust/eframe/egui UI.
- Preserve existing `.morph` / `.morphpart` files, serialized node IDs and port
  keys. Make necessary migrations explicit and verify their compatibility.
- Preserve the MIT OR Apache-2.0 project license and the policy against adopting
  GPL/LGPL/AGPL terms. Retain upstream notices and the exact license choices
  recorded in [docs/DEPENDENCIES.md](docs/DEPENDENCIES.md).
- Inspect the current branch, [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and
  [docs/development/STATUS.md](docs/development/STATUS.md) before changing shared
  architecture. Coordinate changes to the graph model and persistence.
- Use focused development branches and pull requests. Do not push directly to
  `main`, force-push shared history or overwrite another developer's work.
- Record checks actually run and distinguish current results from historical
  evidence. Do not merge unverified changes or report unmeasured performance
  gains, untested compatibility or unfinished features as complete.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the current validation commands.
