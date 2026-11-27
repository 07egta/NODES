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
At10:10:19 JST the Windows full-test step had succeeded and its release
build was running. The whole Windows job is not counted as successful until
a completed result and log have been inspected.

The actual Linux artifact ZIP was downloaded and its entire SHA256 matched
GitHub's artifact digest. The nested tar's BUILD-INFO.txt identifies the
synthetic merge commit. Both executables were checked as little-endian
ELF64/x86_64 and fingerprinted. The downloaded nodes-cli itself ran all three
diagnostic fixtures with the expected exit codes, root/blocked counts and
unchanged input hashes on this Linux/glibc2.41 host. Its raw JSON responses
are retained. The desktop executable was not run as part of this artifact
inspection; the separately recorded native GUI used the local debug build.

These results belong to PR18 source ca6147, not the combined PR16 candidate.
Downloaded ZIPs/executables and temporary signed download URLs are not
committed. Package inspection is not Windows GUI or physical-print testing.

SHA256SUMS covers each file in this directory except itself.
