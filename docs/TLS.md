# Trusted HTTPS

Use this procedure before creating real staff accounts. The school HTTPS
profile publishes only Caddy on the configured HTTPS port. The application
port stays inside the container network, cookies become Secure, and browser
requests must match the configured HTTPS origin.

## Choose a stable name

Choose one school-controlled DNS name that resolves to the host's stable LAN
address. The default configuration uses helpdesk.local, but the operator must
confirm that this exact name is unique and managed on the school network before
using it.

Do not use an IP address unless the certificate contains that IP address as a
subject alternative name. Do not ask staff to bypass a browser warning.

## Create the local certificate authority

The included HTTPS service creates a private local certificate authority and a
certificate for the configured desk name. Its private signing key stays inside
the dedicated Docker volume. The launcher exports only the public root
certificate that client devices need.

Install the desk once from the release directory:

~~~sh runbook-check
./scripts/desk install --host helpdesk.local --name 'School IT Desk' --support 'Call the main office'
~~~

The command creates a private `.env`, builds or downloads the configured image,
starts the application and HTTPS service, waits for both to become healthy, and
writes `exports/local-it-desk-root.crt`.

For an existing installation, export the public certificate again without
changing the authority:

~~~sh runbook-check
./scripts/desk certificate
openssl x509 -in exports/local-it-desk-root.crt -noout -subject -issuer -fingerprint -sha256
~~~

The certificate is safe to distribute to managed client devices. Never copy,
export, or distribute files from the private `desk-caddy-data` Docker volume.

## Establish client trust

Install `exports/local-it-desk-root.crt` through the school's managed trust
system:

- Windows domain clients: deploy to Trusted Root Certification Authorities
  with Group Policy or the school's device manager.
- macOS, iOS, Android, and ChromeOS: deploy a managed trusted-certificate
  profile.
- Firefox: use the managed enterprise trust policy or approved browser
  management.

For a small number of managed Windows devices, an administrator may install the
public root directly from PowerShell:

```powershell
certutil.exe -addstore -f Root .\exports\local-it-desk-root.crt
```

Client devices need only this public certificate. The local signing key is not
exported by the launcher.

From a managed client, open the exact HTTPS_ORIGIN address. Inspect the
certificate in the browser and confirm:

1. No warning or exception appears.
2. The requested DNS name is covered.
3. The issuer is the expected school or approved local CA.
4. The validity dates are current.

Then verify the endpoint:

~~~sh runbook-check
read -r -p 'Help desk DNS name: ' local_it_desk_host
curl --fail --silent --show-error "https://$local_it_desk_host/health/ready"
~~~

Expected result: {"status":"ready"}. Only after a managed client completes
these checks may the administrator create real staff accounts.

## Continuity and replacement

The HTTPS service renews its desk certificate automatically while the
`desk-caddy-data` volume remains intact. Normal application updates preserve
that volume and therefore preserve client trust.

After moving to a new host, losing the Docker volume, or intentionally changing
the desk name, the service creates a new local authority. Export and redeploy
the new public root, then test a managed client again. Do not tell staff to
bypass a warning while trust deployment catches up.

Check service health and re-export the current public root at any time:

~~~sh runbook-check
./scripts/desk status
./scripts/desk certificate
~~~
