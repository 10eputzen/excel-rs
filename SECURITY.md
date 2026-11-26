# Security policy

## Reporting a vulnerability

Please do **not** open a public issue for security problems.

Report them privately through GitHub: on the repository page go to **Security → Report a vulnerability** ([private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability)). Only the maintainers can see the report.

Please include the excel-rs version, your Node version and operating system, how to reproduce it (a sample file helps, with personal data removed) and what an attacker could do. You will get a first answer within a week.

## Scope

excel-rs reads files that usually come from users of your application, so reports about this code are in scope, for example:

- a crafted file that crashes the process, hangs it or makes it allocate far beyond the configured limits (`maxBytes`, `maxUncompressedBytes`, `maxRows`, `maxCells`) instead of failing with an error
- written workbooks where a string ends up as a formula, or output that is not a valid xlsx file

Out of scope: problems in third-party crates that are already fixed upstream (please report those to the crate), and a file that is merely slow to read within the limits.

## Supported versions

Only the latest release and `main` receive fixes.
