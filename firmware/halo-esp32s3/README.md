# Codex Halo ESP32-S3 Firmware

This PlatformIO project hosts the USB CDC protocol and the first hardware-safe
firmware skeleton for the Waveshare ESP32-S3-Touch-AMOLED-1.43.

## Verification

From this directory:

```powershell
python -m platformio test -e native
python -m platformio run -e waveshare_amoled_143
```

The native protocol tests read the shared vectors from
`../../docs/protocol/golden-vectors.tsv`. The firmware protocol accepts at most
512 payload bytes and keeps a fixed 522-byte receive buffer.

No task identity, prompt, response, source code, path, credential, or USB serial
number belongs in device payloads or diagnostics.
