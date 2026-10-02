# Experimental self compiler profiles

This is acceptance research toward T4, **not a shipped measurement backend** and
not a way to make partial artifacts pass policy. It supplies actual compiler cfg
observations for designing target-aware ownership and fact integration.

```sh
nix develop --offline --no-write-lock-file --command \
  python3 acceptance/self_compiler_profile.py --output target/self-cfg-new
python3 -m unittest discover -s acceptance -p 'test_*.py'
```

The script is deliberately restricted to this checkout, a new directory under
`target/`, and POSIX launchers. It runs `cargo check --workspace --all-targets
--locked --offline` in an isolated target directory. This compiles dependencies
and may run their build scripts/procedural macros, just like ordinary self
verification. It does not start daemons, run tests, benchmark or collect coverage.
It must not be described as a source-only or sandboxed operation.

## Evidence captured

A temporary Cargo rustc wrapper records the actual argv for each primary-package
compiler invocation. It calls the same compiler with those arguments plus
`--print=cfg`, then delegates the **unchanged** original compilation command.
The output retains both exit codes, compiler version, relevant flag environment,
entry source, package, crate name, normal/test mode and raw cfg lines. Records are
written independently so parallel Cargo processes cannot interleave JSON writes.

Expected normal/test contexts are enumerated from locked Cargo metadata. Missing,
duplicate, unexpected, failed or empty probes fail validation, as does a mismatch
between `--test` and compiler cfg `test`. Version queries/dependency compiles are
not miscounted as primary units. A clean build directory prevents cache hits from
silently omitting compiler invocations. Source inventories before and after must
match. Existing wrapper environments, unexpected nested compiler wrappers,
feature-gated targets and unmodeled target kinds are not certified by this tool.

`profiles.json` and the raw invocation/Cargo logs are stored under the requested
output directory. An interrupted or failed experiment is not passing evidence.
The first local attempt caught a bare `rustc` executable being incorrectly resolved
relative to cwd; PATH-aware resolution and a regression now cover that case.

## Observed self profile

Pinned Linux Rust 1.95 produced all **11** expected contexts: four normal/test
pairs (rqlens, helper library, rust_facts, ast_hasher) and three integration-test
roots. Normal contexts reported 19 cfg entries; test contexts reported 20,
including `test`. No source changes occurred. See
`acceptance/self-compiler-profile-results.json`.

These results show why one syntax target label cannot certify a compiled method
inventory across normal and test builds of the same source root.

## Not established

- No Rustdoc substitute, unstable compiler flag or `RUSTC_BOOTSTRAP` override is
  introduced. The experiment preserves any inherited flag environment in its
  evidence rather than silently substituting a different compiler mode.
- Cfg strings are not yet evaluated against source attributes or merged with
  expansion results. No statement of inactive/active source coverage follows.
- Entry roots and compiler arguments do not supply a complete module membership
  graph, def IDs, macro hygiene, generated source maps or resolved impl owners.
- This currently studies the self workspace's default-feature check profile.
  It does not implement arbitrary feature/build-script target profiles or certify
  the declared macOS/Windows/latest-stable integration matrix.
- No `RunContext`, source scope, scoring model, policy gate or artifact confidence
  is changed by the experiment. Required cfg/owner/generated-fact integration
  remains outstanding; **T4 is still failed**.
