<div align="center">
  <a href="README.md"><img src="assets/Miru.png" alt="MiruScriptX" height="44"></a>
</div>

# Security Policy

## Supported versions

MiruScriptX is an early-stage project. Security fixes are applied to the latest
release on the default branch. Older versions are not maintained.

## Reporting a vulnerability

Please report security issues privately, not through public issues.

- Preferred: open a private security advisory with the "Report a vulnerability"
  button on the repository's Security tab.
- Alternatively, email the maintainer at stivenagostingjekaj@gmail.com.

Please include steps to reproduce, the affected version or commit, and the
impact as you understand it. You can expect an initial response within a few
days. Once a fix is ready it will be released, and your report will be
acknowledged unless you prefer to remain anonymous.

## Scope

MiruScriptX runs the programs you give it with the full trust of your user
account. It is an interpreter, not a sandbox: a `.miru` program can do anything
the `miru` process can do. Do not run untrusted MiruScriptX code expecting
isolation. Reports about the lack of sandboxing are out of scope, since that is
a known and documented property rather than a vulnerability.
