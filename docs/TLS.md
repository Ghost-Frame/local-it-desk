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

## Obtain the certificate

Request a server certificate and private key from the school certificate
authority or the approved local certificate process. The certificate must:

- contain the selected DNS name in subjectAltName;
- permit TLS server authentication;
- include any intermediate certificates after the leaf certificate;
- remain valid for the planned operating period;
- use a private key kept only by authorized operators.

Put the PEM files at these exact release-relative paths:

- certs/tls.crt
- certs/tls.key

On a Linux Docker host, restrict the key while allowing the non-root proxy to
read it:

~~~sh runbook-check
mkdir -p certs
sudo chown root:10001 certs/tls.crt certs/tls.key
sudo chmod 0644 certs/tls.crt
sudo chmod 0640 certs/tls.key
~~~

If the platform uses user-namespace mapping, confirm access with the Compose
configuration test below. Do not weaken the key to world-writable permissions.

## Validate the certificate and key

Enter the selected DNS name when prompted:

~~~sh runbook-check
read -r -p 'Help desk DNS name: ' local_it_desk_host
test -n "$local_it_desk_host"
openssl x509 -in certs/tls.crt -noout -checkend 2592000
openssl x509 -in certs/tls.crt -noout -ext subjectAltName |
  grep -F "$local_it_desk_host"
certificate_key_hash="$(openssl x509 -in certs/tls.crt -pubkey -noout |
  openssl pkey -pubin -outform DER |
  sha256sum |
  awk '{print $1}')"
private_key_hash="$(openssl pkey -in certs/tls.key -pubout -outform DER |
  sha256sum |
  awk '{print $1}')"
test "$certificate_key_hash" = "$private_key_hash"
~~~

Expected result: the certificate remains valid for at least 30 days, the DNS
name appears in subjectAltName, and the public-key hashes match. Stop if any
command fails.

## Configure and start HTTPS

Create .env from the supplied example if it does not exist, then set the exact
origin without adding a path:

~~~sh runbook-check
test -f .env || cp .env.example .env
read -r -p 'Help desk DNS name: ' local_it_desk_host
test -n "$local_it_desk_host"
awk -v origin="https://$local_it_desk_host" \
  'BEGIN { replaced=0 } /^HTTPS_ORIGIN=/ { print "HTTPS_ORIGIN=" origin; replaced=1; next } { print } END { if (!replaced) exit 1 }' \
  .env > .env.next
mv .env.next .env
docker compose -f compose.yaml -f compose.https.yaml config --quiet
docker compose -f compose.yaml -f compose.https.yaml up --detach
docker compose -f compose.yaml -f compose.https.yaml ps
~~~

Expected result: app and caddy become healthy. If Caddy reports a permission
failure for tls.key, correct host ownership or user-namespace mapping. Do not
relax the file beyond read access required by container user 10001.

## Establish client trust

Install the issuing root and any required intermediate CA certificates through
the school's managed trust system:

- Windows domain clients: deploy to Trusted Root Certification Authorities
  with Group Policy or the school's device manager.
- macOS, iOS, Android, and ChromeOS: deploy a managed trusted-certificate
  profile.
- Firefox: use the managed enterprise trust policy or install the school root
  through approved browser management.

Never distribute tls.key. Client devices need only the public CA certificate
chain.

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

## Renewal

Track certificate expiration in the school's normal monitoring calendar.
Renew at least 30 days before expiry. Validate the replacement pair, replace
only certs/tls.crt and certs/tls.key, then recreate caddy:

~~~sh runbook-check
docker compose -f compose.yaml -f compose.https.yaml up --detach --force-recreate caddy
docker compose -f compose.yaml -f compose.https.yaml ps
~~~

Test from a managed client again. Keep the old certificate and key only through
the school's protected recovery process, then follow the school's retention
policy.
