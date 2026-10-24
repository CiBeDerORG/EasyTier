"""Lossless UTF-8 credentials, with compatibility for existing Bearer clients."""
import base64
import binascii
import secrets


def authorization(token: str) -> str:
    if all(0x21 <= byte <= 0x7e for byte in token.encode()):
        return "Bearer " + token
    return "AutomationToken " + base64.b64encode(token.encode()).decode("ascii")


def authenticated(supplied: str, token: str) -> bool:
    try:
        if supplied.startswith("AutomationToken "):
            value = base64.b64decode(supplied[len("AutomationToken "):], validate=True)
        elif supplied.startswith("Bearer "):
            # ASGI presents raw HTTP header bytes decoded with Latin-1.
            value = supplied[len("Bearer "):].encode("latin-1")
        else:
            return False
    except (ValueError, UnicodeError, binascii.Error):
        return False
    return secrets.compare_digest(value, token.encode())
