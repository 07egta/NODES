# Corrective PR18 CI and development artifact verification

This record follows the real Windows failure on aa690657. The correction is
head ca6147a0b639b9429900313147d4e288972373e5. GitHub checks out its synthetic
PR merge d3128a29f45f142873ca0ba8a0d2d72d1161d924; both have exactly tree
55903180e8609378657c00095833c295b9e4caf9. The merge parents and source-specific
job state are recorded in ci-results.json.

Linux completed all gates, 487/0/2 across44 Rust test suites, release
desktop/CLI builds and artifact upload. Exact selected log lines retain
the commands, compiler, test counts and the two extreme-angle regressions.
The excerpt omits setup/dependency installation and links the original job.
At10:14:14 JST the whole run and both jobs were completed successfully.
Windows passed486/0/2 across44 suites, followed by release builds and artifact
upload. Its exact log excerpt includes both extreme-angle regressions that
were central to the correction. The single Linux-only passing test is the
Unix FIFO input case; the source uses #[cfg(unix)]. Other passing test names
agree. Prior pending observations are historical, not the final state.

The actual Linux artifact ZIP was downloaded and its entire SHA256 matched
GitHub's artifact digest. The nested tar's BUILD-INFO.txt identifies the
synthetic merge commit. Both executables were checked as little-endian
ELF64/x86_64 and fingerprinted. The downloaded nodes-cli itself ran all three
diagnostic fixtures with the expected exit codes, root/blocked counts and
unchanged input hashes on this Linux/glibc2.41 host. Its raw JSON responses
are retained. The desktop executable was not run as part of this artifact
inspection; the separately recorded native GUI used the local debug build.

The actual Windows ZIP also matched the GitHub SHA256 digest. Its
BUILD-INFO.txt names the same synthetic merge. Both binaries have valid
MZ/PE headers, PE32+ optional-header magic and x86_64 machine0x8664; fingerprints
and imported DLL names are recorded in windows-inspection.json. Both import
VCRUNTIME140.dll. No Windows executable or installer was run on this Linux
host, and no Windows GUI claim is made. The imported PR16 documentation at
[WINDOWS_RUNTIME.md](../../WINDOWS_RUNTIME.md) describes the runtime requirement.

The runner warned that pinned actions target Node.js20 and are forced onto
Node.js24. The run nevertheless passed; updating immutable action pins is a
separate maintenance task and was not mixed into this numerical fix.

These results belong to PR18 source ca6147, not the combined PR16 candidate.
Downloaded ZIPs/executables and temporary signed download URLs are not
committed. Package inspection is not Windows GUI or physical-print testing.

SHA256SUMS covers each file in this directory except itself.
