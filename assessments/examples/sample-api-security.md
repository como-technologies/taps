# Assessment: API Security Readiness

> Evaluate the security posture of REST API implementations to identify vulnerabilities and compliance gaps before production deployment.

**Goal:** Ensure APIs meet security best practices and protect against OWASP Top 10 API threats.

---

## Domain: Authentication & Authorization

**Context:** How API access is controlled and users/systems are verified and granted permissions
**Value:** Prevents unauthorized access, protects sensitive data, ensures compliance with access control requirements (SOC2, ISO 27001)
**Risk:** Data breaches, privilege escalation, unauthorized operations, regulatory violations, credential theft

### Practice: Token-Based Authentication

**Context:** Using OAuth 2.0 or JWT tokens for stateless API authentication
**Value:** Industry-standard security, eliminates session storage, enables secure delegation, supports microservices architecture
**Risk:** Weak authentication allows credential theft, session hijacking, replay attacks, token forgery

#### Questions

- [ ] Are all API endpoints (except explicitly public ones) protected with token-based authentication?
  - *Check API gateway configuration; verify endpoints require OAuth 2.0 or JWT tokens, not basic auth or other methods*

- [ ] Do access tokens expire within a reasonable timeframe (≤1 hour)?
  - *Review token configuration in authentication service; shorter expiration reduces impact of token theft*

- [ ] Are refresh tokens stored securely?
  - *Evidence: Encrypted database, secrets manager (HashiCorp Vault, AWS Secrets Manager), not in application logs*

- [ ] Are refresh tokens rotated regularly?
  - *Evidence: Rotation policy documentation, automatic rotation on use or time-based expiration*

- [ ] Are JWT signatures verified on every request?
  - *Verify middleware validates signature using proper algorithm (RS256/ES256, not HS256 with weak secrets)*

- [ ] Is the 'none' algorithm disabled for JWT?
  - *Critical: Ensure JWT library configuration rejects unsigned tokens*

### Practice: Role-Based Access Control (RBAC)

**Context:** Granular permission system based on user roles and resource ownership
**Value:** Implements principle of least privilege, prevents lateral movement, supports compliance audits, simplifies permission management
**Risk:** Over-privileged users, unauthorized data access, compliance failures, insider threats

#### Questions

- [ ] Is RBAC implemented for all API operations (not just authentication)?
  - *Verify each endpoint checks permissions, not just authentication status*

- [ ] Are roles defined with minimum necessary permissions?
  - *Review role definitions against least-privilege principle; avoid overly broad roles*

- [ ] Can users only access resources they own or have explicit permission for?
  - *Test with different user accounts across resource boundaries; check for IDOR vulnerabilities*

- [ ] Are administrative functions restricted to admin roles?
  - *Verify user management, configuration changes, data deletion require elevated privileges*

### Practice: API Key Management

**Context:** Secure handling of API keys for service-to-service authentication
**Value:** Enables secure integrations, supports rate limiting, provides audit trail
**Risk:** Leaked keys allow unauthorized access, difficult to rotate compromised keys, abuse of API resources

#### Questions

- [ ] Are API keys treated as secrets and never committed to source control?
  - *Scan repositories for leaked keys; use tools like git-secrets or Trufflehog*

- [ ] Can API keys be rotated without downtime?
  - *Test key rotation process; verify support for multiple active keys during transition*

- [ ] Are API keys scoped to minimum necessary permissions?
  - *Each key should be limited to specific operations/resources needed*

- [ ] Is there monitoring and alerting for unusual API key usage?
  - *Evidence: Rate limiting, geographic anomalies, usage spikes trigger alerts*

---

## Domain: Data Protection

**Context:** How sensitive data is secured in transit, at rest, and during processing
**Value:** Protects confidentiality and integrity, meets compliance requirements (GDPR, HIPAA, PCI-DSS), maintains customer trust
**Risk:** Data exposure, compliance violations, financial penalties, loss of customer trust, competitive disadvantage

### Practice: Encryption in Transit

**Context:** All API communication uses TLS/HTTPS with modern cryptographic standards
**Value:** Prevents eavesdropping, man-in-the-middle attacks, data tampering, ensures data integrity
**Risk:** Credentials and data transmitted in plaintext, interception attacks, session hijacking

#### Questions

- [ ] Do all API endpoints enforce HTTPS/TLS 1.2 or higher?
  - *Check server configuration; verify HTTP redirects to HTTPS or is completely disabled*

- [ ] Are TLS certificates valid and from trusted Certificate Authorities?
  - *Verify certificate chain, expiration dates, no self-signed certificates in production*

- [ ] Is HTTP Strict Transport Security (HSTS) enabled?
  - *Check response headers for HSTS directive with appropriate max-age*

- [ ] Are weak cipher suites disabled?
  - *Use SSL Labs or testssl.sh to verify only strong ciphers (AES-GCM, ChaCha20) are enabled*

### Practice: Sensitive Data Handling

**Context:** PII, credentials, payment data, and confidential information are properly protected throughout their lifecycle
**Value:** Compliance with privacy regulations, prevents data breaches, protects user privacy
**Risk:** Regulatory fines ($millions for GDPR violations), lawsuits, reputational damage, customer data exposure

#### Questions

- [ ] Is sensitive data encrypted at rest?
  - *Check database encryption (TDE), file system encryption, secrets management for API keys/passwords*

- [ ] Are passwords hashed with modern algorithms (bcrypt, Argon2, scrypt)?
  - *Review authentication code; verify no plaintext or weak hashing (MD5, SHA1, plain SHA256)*

- [ ] Is sensitive data excluded from logs and error messages?
  - *Audit logging configuration; check error responses don't leak PII, tokens, or internal details*

- [ ] Are API keys/secrets stored in secure vaults (not in code)?
  - *Verify use of secrets manager (HashiCorp Vault, AWS Secrets Manager); no hardcoded credentials in repositories*

- [ ] Is sensitive data masked in non-production environments?
  - *Development/staging should use anonymized data, not production data dumps*

### Practice: Data Validation & Sanitization

**Context:** All input is validated and sanitized to prevent injection attacks
**Value:** Protects against SQL injection, XSS, command injection, path traversal attacks
**Risk:** Database compromise, remote code execution, data exfiltration, system takeover

#### Questions

- [ ] Is all user input validated against expected format/type?
  - *Use schema validation (JSON Schema, OpenAPI) and type checking*

- [ ] Are parameterized queries or ORMs used to prevent SQL injection?
  - *Verify no string concatenation in database queries*

- [ ] Is output encoding applied to prevent XSS in API responses?
  - *Especially important for APIs consumed by web frontends*

- [ ] Are file uploads validated for type, size, and scanned for malware?
  - *Check file extension validation, content-type verification, size limits, virus scanning*

---

## Domain: API Design & Configuration

**Context:** How the API is architected and configured to minimize attack surface
**Value:** Reduces vulnerabilities, improves security posture, makes secure usage the default path
**Risk:** Information disclosure, abuse, denial of service, unintended functionality exposure

### Practice: Rate Limiting & Throttling

**Context:** Restricting the number of requests per client over time windows
**Value:** Prevents brute force attacks, DoS/DDoS, resource exhaustion, reduces costs from abuse
**Risk:** Credential stuffing, account takeover, service degradation, excessive cloud costs

#### Questions

- [ ] Are rate limits implemented on authentication endpoints?
  - *Critical for preventing brute force attacks; enforce strict limits (e.g., 5 attempts per minute)*

- [ ] Are rate limits applied to resource-intensive operations?
  - *Search, report generation, bulk operations should have stricter limits*

- [ ] Do rate limit responses include retry-after headers?
  - *Return 429 status with Retry-After header for client guidance*

- [ ] Are rate limits configurable per client or API key?
  - *Allow different tiers (free, premium) with appropriate limits*

### Practice: Error Handling & Information Disclosure

**Context:** Error responses are secure and don't leak system internals
**Value:** Prevents reconnaissance, makes exploitation harder, maintains security through appropriate opacity
**Risk:** Stack traces reveal technology stack, error messages expose database structure, verbose errors aid attackers

#### Questions

- [ ] Do error responses avoid exposing stack traces in production?
  - *Return generic messages to clients; log detailed errors server-side only*

- [ ] Are HTTP status codes used appropriately without leaking information?
  - *Use 401 for unauthenticated, 403 for unauthorized, avoid distinguishing valid/invalid usernames*

- [ ] Is debugging mode disabled in production?
  - *Verify no verbose error pages, debug endpoints, or test data in production*

- [ ] Are internal system details excluded from responses?
  - *Don't expose server versions, frameworks, database types, internal IPs in headers or errors*

### Practice: CORS & Cross-Origin Security

**Context:** Cross-Origin Resource Sharing configuration restricts which domains can access the API
**Value:** Prevents unauthorized cross-origin requests, protects against CSRF-like attacks, controls API access
**Risk:** Unauthorized domains can access API, data exfiltration, cross-site request forgery

#### Questions

- [ ] Is CORS configured with explicit allowed origins (not wildcard)?
  - *Avoid Access-Control-Allow-Origin: * in production; use specific domains*

- [ ] Are credentials (cookies, auth headers) only allowed from trusted origins?
  - *If using Access-Control-Allow-Credentials: true, origin must be specific*

- [ ] Are preflight requests properly handled?
  - *Verify OPTIONS requests return appropriate CORS headers*

- [ ] Is the API protected against CSRF attacks?
  - *Use anti-CSRF tokens for state-changing operations if using cookie-based auth*

---

## Domain: Monitoring & Incident Response

**Context:** Detecting, logging, and responding to security events and anomalies
**Value:** Early threat detection, forensic capabilities, compliance requirements, rapid incident response
**Risk:** Undetected breaches, prolonged attacker access, inability to investigate incidents, compliance violations

### Practice: Security Logging & Auditing

**Context:** Comprehensive logging of security-relevant events for monitoring and forensics
**Value:** Enables threat detection, supports investigations, meets compliance audit requirements (SOC2, PCI-DSS)
**Risk:** Blind to attacks, cannot investigate incidents, fails compliance audits, delayed breach detection

#### Questions

- [ ] Are authentication attempts (success and failure) logged?
  - *Include timestamp, username/ID, source IP, user agent for forensics*

- [ ] Are authorization failures logged?
  - *Track who tried to access what they shouldn't; may indicate reconnaissance or privilege escalation*

- [ ] Are logs centralized and retained for adequate periods?
  - *Use centralized logging (ELK, Splunk); retain per compliance requirements (typically 90+ days)*

- [ ] Are logs monitored for suspicious patterns?
  - *Evidence: Automated alerting on brute force, unusual access patterns, privilege escalation attempts*

- [ ] Are logs protected from tampering and unauthorized access?
  - *Use write-once storage or SIEM; restrict log access to security team*

### Practice: Vulnerability Management

**Context:** Regular assessment and remediation of security vulnerabilities
**Value:** Reduces attack surface, stays ahead of emerging threats, demonstrates due diligence
**Risk:** Exploitation of known vulnerabilities, zero-day attacks, compromised dependencies

#### Questions

- [ ] Are dependencies scanned for known vulnerabilities?
  - *Use tools like Snyk, Dependabot, npm audit; have process for urgent patches*

- [ ] Is the API tested with security scanning tools?
  - *SAST (static analysis), DAST (dynamic testing), API security scanners (OWASP ZAP, Burp)*

- [ ] Is there a process for emergency security patches?
  - *Can deploy critical fixes within hours, not days; tested rollback procedures*

- [ ] Are security assessments conducted regularly?
  - *Annual penetration tests, quarterly vulnerability scans, continuous automated scanning*

### Practice: Incident Response Preparedness

**Context:** Plans and procedures for responding to security incidents
**Value:** Minimizes damage, reduces recovery time, maintains stakeholder confidence, meets compliance
**Risk:** Chaotic response, extended downtime, regulatory penalties, loss of evidence

#### Questions

- [ ] Is there a documented incident response plan?
  - *Includes roles, escalation procedures, communication templates, technical runbooks*

- [ ] Can the API be quickly isolated if compromised?
  - *Test ability to disable specific services, revoke credentials, block traffic*

- [ ] Are security incidents tracked and reviewed?
  - *Post-incident reviews to identify improvements; maintain incident log*

- [ ] Are relevant teams trained on security incident procedures?
  - *Development, operations, security teams know their roles; annual tabletop exercises*
