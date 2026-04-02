# Security

## Trust Model

- Every daemon instance owns a persistent device identity.
- Pairing requires a short-lived one-time code plus local user approval.
- Trusted peers are stored locally and pinned by device ID and public key.
- Untrusted peers must never be allowed to inject input.

## Threat Posture

HyprBridge assumes a local network but does not trust it blindly. Discovery can happen over LAN, but session establishment and control commands must remain authenticated and encrypted.

The current transport runtime uses:

- persisted self-signed QUIC certificates per daemon instance
- mDNS advertisement of the certificate digest and bootstrap certificate material
- trust-store checks before accepting inbound sessions

This gives the MVP an encrypted, device-aware session layer while leaving room to harden certificate pinning and remote pairing ergonomics further.
