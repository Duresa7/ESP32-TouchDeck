# The Electron updater keeps the existing signing key

v0.2.0 moves to the Electron app through its own Tauri updater: the first Electron release's installer is signed with the existing updater key and listed in the update manifest v0.2.0 already reads. Layouts and settings carry over.

From then on the Electron updater also checks that same signature before installing. A stock electron-updater setup without a code-signing certificate checks only a checksum published next to the installer, so anyone who could change a GitHub release could ship an update. With the separate key, a hijacked release alone can't. Installers stay without an Authenticode signature until there's a certificate.

Never regenerate or replace the key. Every installed copy trusts it.
