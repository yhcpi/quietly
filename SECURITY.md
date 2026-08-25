# Security Policy

## Supported versions

Only the latest release of Quietly receives security fixes. Older versions
are not patched.

## Reporting a vulnerability

Please **do not open a public issue** for security vulnerabilities.

Report privately via one of:

- GitHub Security Advisories: use the "Report a vulnerability" button on
  the Security tab of this repository.
- Email: see the repository owner's profile for a contact address.

Include:

- A clear description of the vulnerability and its impact
- Reproduction steps (a minimal config / rule set is enough)
- Affected version (commit hash or release tag)
- Your assessment of severity, if any

## Response timeline

We aim to:

- Acknowledge reports within **3 business days**
- Provide a triage assessment within **10 business days**
- Coordinate a fix and disclosure timeline case-by-case

## Threat model

Quietly runs a local HTTP/HTTPS forward proxy bound to `127.0.0.1` and writes
to `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings` while
a focus session is active. Out-of-scope for security reports:

- Browsers that do not honor the HKCU proxy (Firefox, Tor) — these are
  documented limitations, not vulnerabilities.
- The HTTPS block page being a connection error rather than a custom HTML
  page — this is a Chromium CONNECT-tunnel protocol constraint.

In scope:

- Anything reachable from a malicious block-rule pattern (regex ReDoS,
  pattern-driven resource exhaustion, etc.).
- Anything that lets a non-admin process on the host affect another user's
  browser through the proxy.
- Anything that prevents the user's original proxy settings from being
  restored on crash or abnormal exit (a "stuck proxy" scenario).