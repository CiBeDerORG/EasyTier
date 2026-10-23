# Media adapter sources

- `h264_track.py` adapts `CiBeDerORG/EasyADB/server/m2_webrtc.py`, read from local revision `dad5f4063fc737e9740273b1e8cf27185505d86c`. The original pre-encoded PyAV Packet path and bounded cross-thread delivery are retained. This integration adds a bounded frame age / keyframe wait indicator. No license declaration was found at the EasyADB repository root in the reviewed checkout; this notice records provenance without assigning a new upstream license.
- `scrcpy_source.py` is this project's reduced main-display adapter, implemented against the same pinned scrcpy 5.0.1 protocol referenced by EasyADB's `m2_live.py`, `m2_touch.py`, `m2_keys.py`, and `m2_video_probe.py`. It does not import EasyADB's device store, global token, unlocking, experiments, or virtual display features.
- Android server protocol: [Genymobile/scrcpy v5.0.1](https://github.com/Genymobile/scrcpy/tree/v5.0.1), especially `server/.../device/Streamer.java`, `server/.../control/ControlMessageReader.java`, and `app/src/control_msg.c`. The upstream Apache-2.0 license is preserved in `SCRCPY-LICENSE`, copied from the local upstream source checkout.
- The Android server binary is external to this source tree. `AUTOMATION_SCRCPY_SERVER` must point to the official v5.0.1 server with SHA-256 `764eb6f79811d5211fe9df341120882ba9994c7a61b897d7bf3fb662e53bc536`. The executor checks this digest before launching it. No runtime download is performed.

The main-display adapter requests an encoder I-frame interval of two seconds using scrcpy's codec options. This is a requested codec setting, not a guarantee that every device honors it, nor an implementation of PLI-to-phone feedback. Track queues remain bounded and fail a session when keyframe recovery exceeds its deadline.

These are source/protocol references, not evidence of integration or device testing. aiortc, PyAV and scrcpy retain their respective upstream notices in their distributions.
