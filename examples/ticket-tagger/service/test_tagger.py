"""The reference service's own checks: `python3 -m unittest discover service` from the example."""
import hashlib
import hmac
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(__file__))
import tagger  # noqa: E402


def sign(secret, ts, body):
    return f"t={ts},v1=" + hmac.new(secret.encode(), f"{ts}.".encode() + body, hashlib.sha256).hexdigest()


class Tagger(unittest.TestCase):
    def test_a_signature_is_checked_and_must_be_fresh(self):
        body = b'{"entity":"Ticket","id":"1","event":"created"}'
        good = sign("s3cret", 1000, body)
        self.assertTrue(tagger.signed("s3cret", good, body, now=1000))
        self.assertFalse(tagger.signed("other", good, body, now=1000), "another secret")
        self.assertFalse(tagger.signed("s3cret", good, body + b" ", now=1000), "another body")
        self.assertFalse(tagger.signed("s3cret", good, body, now=2000), "too old")
        self.assertFalse(tagger.signed("s3cret", "garbage", body, now=1000))

    def test_topics_from_words_without_a_model(self):
        os.environ.pop("ANTHROPIC_API_KEY", None)
        self.assertEqual(tagger.classify("Refund for invoice 42", ""), "Billing")
        self.assertEqual(tagger.classify("Cannot sign in", "my password is not accepted"), "Account")
        self.assertEqual(tagger.classify("The app crashes", ""), "Bug")
        self.assertEqual(tagger.classify("Opening hours?", ""), "Question")
        self.assertEqual(tagger.classify("Hello", ""), "Other")


if __name__ == "__main__":
    unittest.main()
