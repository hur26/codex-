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

## 硬件安全提示

- 第一轮实体 bring-up 只允许连接一圈 20 LED 的 5 V WS2813 兼容灯环。
- 固件保持 `maxMilliAmps=500`，亮度硬上限保持 30%，直到真实电流和温升实测通过。
- 使用带限流保护的 5 V 电源、共地、靠近第一颗 LED 的 330-470 ohm 数据电阻，以及跨接 LED 5 V/GND 的 500-1000 uF 电容。
- 开发板通过 USB-C 供电和通信；灯环使用外接 5 V 电源。未经板卡原理图和实测确认，不把外接 5 V 接入开发板 5 V/USB VBUS。
- 不假定 5 V 灯环能够可靠识别 3.3 V 数据；首选 SN74AHCT125/74AHCT125 或经验证兼容 WS281x 时序的 5 V 单向缓冲模块，并按完整清单采用固定 `OE`、约 10 kΩ 数据输入下拉、去耦和未用通道终止方案。
- 独立供电时先开启 LED/缓冲器 5 V、再连接 MCU/USB；下电时先停止 DATA 并设为高阻，再断 MCU/USB，最后断 LED/缓冲器 5 V。灯环未供电时禁止驱动 DATA。
- `maxMilliAmps` 和亮度上限不能替代外部限流和正确接线；发生异常闪烁、重启或发热时立即断电。
- 不得从开发板 5 V 引脚给四圈 LED 供电。
- 接线前先阅读[最小采购 BOM](../../docs/hardware/2026-07-29-prototype-bom-v0.1.md)和[接线与供电检查清单](../../docs/hardware/2026-07-29-wiring-and-power-checklist.md)。
