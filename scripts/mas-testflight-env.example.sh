#!/bin/bash
# Example only. Copy to mas-testflight-env.sh (gitignored), fill values, then:
#   source scripts/mas-testflight-env.sh && ./scripts/build-mas-testflight.sh
#
# Certificate names: run  security find-identity -v -p codesigning  (app)
# Installer line is often only in:     security find-identity -v -p basic
#
# Application (sign the .app):
#   Apple Distribution: Your Team (TEAMID)
# Installer (sign the .pkg):
#   3rd Party Mac Developer Installer: Your Team (TEAMID)
#   (If Xcode shows a different "Mac Installer Distribution" label, use the exact string from find-identity.)

# export MAS_APP_SIGN_IDENTITY='Apple Distribution: Your Team (XXXXXXXXXX)'
# export MAS_INSTALLER_SIGN_IDENTITY='3rd Party Mac Developer Installer: Your Team (XXXXXXXXXX)'
# export BUNDLE_ID='app.chinotto'
# Optional build-only override. Increment for every replacement upload of the same marketing version.
# export MAS_BUILD_NUMBER='3.0.1'
# fastembed cache copied into the app so the MAS build never downloads feature data at runtime.
# export MAS_MODEL_CACHE="$HOME/Library/Application Support/app.chinotto/models"
# App Store / TestFlight: download a Mac **App Store** provisioning profile for this App ID, then:
# export MAS_PROVISIONING_PROFILE="$HOME/Downloads/Chinotto_mac_app_store.provisionprofile"
