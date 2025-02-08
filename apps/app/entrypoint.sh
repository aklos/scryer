#!/bin/sh

set -e

# Install GeoIP update package
dpkg -i geoipupdate.deb

# Ensure necessary environment variables are set
if [ -z "$GEOIP_ACCOUNT_ID" ] || [ -z "$GEOIP_LICENSE_KEY" ]; then
  echo "Error: GEOIP_ACCOUNT_ID or GEOIP_LICENSE_KEY is not set."
  exit 1
fi

# Create GeoIP.conf file
cat <<EOF > /etc/GeoIP.conf
# GeoIP.conf file - used by geoipupdate program to update databases
AccountID $GEOIP_ACCOUNT_ID
LicenseKey $GEOIP_LICENSE_KEY
EditionIDs GeoLite2-Country
EOF

# Run geoipupdate
geoipupdate

# Start the application
exec node apps/app/server.js
