import unittest
from automation_executor.auth import authorization, authenticated


class CredentialTransportTests(unittest.TestCase):
    def test_legacy_ascii_tokens_keep_bearer_compatibility(self):
        token = 'existing-token-with-punctuation!$`"123456789'
        self.assertEqual(authorization(token), "Bearer " + token)
        self.assertTrue(authenticated(authorization(token), token))

    def test_spaces_quotes_and_unicode_round_trip_without_normalization(self):
        for token in ['  中文 token ! $ ` " 123456789123456789  ', 'spaces within credential 123456789123456789', 'x' * 32 + ' ']:
            encoded = authorization(token)
            self.assertTrue(encoded.startswith('AutomationToken '))
            self.assertTrue(encoded.isascii())
            self.assertTrue(authenticated(encoded, token))
            self.assertFalse(authenticated(encoded, token + 'x'))
            if token != token.strip():
                self.assertFalse(authenticated(encoded, token.strip()))

    def test_malformed_and_wrong_schemes_fail_without_exceptions(self):
        for supplied in ['', 'Basic xyz', 'AutomationToken !!!', 'AutomationToken 中文', 'Bearer short']:
            self.assertFalse(authenticated(supplied, 'secret-' * 8))
