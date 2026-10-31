# Security policy

## Reporting

Do not publish credentials, Cookie files, authentication tokens, personal data,
or an exploitable vulnerability in a public issue.

Use GitHub's private security advisory flow for the repository:

<https://github.com/bill810975/thu-cli/security/advisories/new>

Include the affected command and version, impact, reproduction steps with
secrets redacted, and any suggested mitigation.

## Sensitive local state

The CLI can persist authenticated Cookies and, only when explicitly requested
on macOS, a Keychain credential. Treat all of the following as secrets:

- session Cookie stores;
- password files;
- MFA and CAPTCHA responses;
- imported browser Cookies;
- command arguments containing tokens;
- downloaded account data.

Never attach those files to an issue or log them in CI.

## Scope

Local-only exploitation requiring an already-compromised user account may be
reviewed, but is lower priority than vulnerabilities that expose credentials,
cross an authentication boundary, permit unintended mutations, or allow
untrusted redirects and downloads.
