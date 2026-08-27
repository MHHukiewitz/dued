# Security policy

## Supported versions

The `main` branch and the latest published `dued` release are in scope.

## What to report

Report a vulnerability when `dued` can:

- Read files outside the target repository during a normal scan
- Execute untrusted code from the scanned repository
- Exfiltrate source or credentials
- Write outside `dued/` without a documented command

Do not file a public GitHub issue for these reports.

## How to report

Use GitHub private vulnerability reporting:

<https://github.com/MHHukiewitz/dued/security/advisories/new>

If that form is not available, contact the maintainer through the
[GitHub profile](https://github.com/MHHukiewitz).

Include:

- Affected version or commit
- Steps to reproduce
- What you expected
- What happened
- A suggested fix if you have one

## What we will do

The maintainer will acknowledge the report and work on a fix before any public
disclosure. Please give time for a patch and a release.

## Local analysis

`dued` is designed to keep source on the local machine. It does not call a
third-party analysis API. The default embed model may download weights into the
local Hugging Face cache on first use. Tests and CI must set `DUED_STUB_EMBED=1`
and should pass `--no-embed` when they do not need vectors.
