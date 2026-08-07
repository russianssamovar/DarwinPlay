# Security

DarwinPlay executes Windows software and should treat all imported executables as untrusted.

The runtime removes the default Wine `Z:` mapping when creating prefixes. Manually imported games receive only an explicit `G:` mapping to their executable directory. Steam receives the normal Wine `C:` drive inside its dedicated prefix and can use additional drives only when mappings exist in that prefix.

The Steam installer is downloaded over HTTPS from the Steam CDN with redirects restricted to HTTPS. The downloaded file is validated as a PE executable before Wine runs it. Users can provide a local installer explicitly instead of downloading one.

DarwinPlay never accepts Steam credentials, Steam Guard codes or session tokens. Authentication is performed entirely in the official Windows Steam client running under Wine.

Runtime component updates should be distributed with cryptographic manifests before enabling unattended component updates. DXMT installation currently requires an explicit local package selected by the user.
