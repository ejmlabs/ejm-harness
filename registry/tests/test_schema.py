"""Exercise local SQLite constraints used by D1. No cloud calls."""

import pathlib
import sqlite3
import unittest


class RegistrySchema(unittest.TestCase):
    def setUp(self) -> None:
        self.db = sqlite3.connect(":memory:")
        migration = pathlib.Path(__file__).parents[1] / "migrations/0001_registry.sql"
        self.db.executescript(migration.read_text())
        self.db.execute("INSERT INTO registry_accounts(id) VALUES ('alice'),('bob'),('operator')")
        self.db.execute(
            "INSERT INTO publishers(handle,user_id,display_name) VALUES ('alice','alice','Alice')"
        )
        self.db.commit()

    def tearDown(self) -> None:
        self.db.close()

    def reserve(self, name: str = "test", version: str = "1.0.0", size: int = 128) -> None:
        self.db.execute(
            "INSERT INTO versions(publisher,name,version,manifest_json,description,sha256,size) "
            "VALUES ('alice',?,?, '{}','Fixture',?,?)",
            (name, version, "a" * 64, size),
        )

    def test_identity_cannot_change_or_be_reused(self) -> None:
        self.reserve()
        for statement in [
            "UPDATE publishers SET handle='stolen' WHERE handle='alice'",
            "UPDATE publishers SET user_id='bob' WHERE handle='alice'",
            "UPDATE versions SET sha256='forged'",
            'UPDATE versions SET manifest_json=\'{"name":"forged"}\'',
            "UPDATE versions SET version='2.0.0'",
            "DELETE FROM versions",
            "INSERT INTO publishers(handle,user_id,display_name) VALUES ('alice','bob','Bob')",
            (
                "INSERT INTO publishers(handle,user_id,display_name) "
                "VALUES ('another','alice','Alice')"
            ),
        ]:
            with self.subTest(statement=statement), self.assertRaises(sqlite3.IntegrityError):
                self.db.execute(statement)

    def test_suspension_blocks_claim_reserve_and_finalization(self) -> None:
        self.reserve()
        self.db.execute("UPDATE registry_accounts SET suspended=1 WHERE id='bob'")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute(
                "INSERT INTO publishers(handle,user_id,display_name) VALUES ('bob','bob','Bob')"
            )
        for suspend, restore in [
            (
                "UPDATE publishers SET suspended=1 WHERE handle='alice'",
                "UPDATE publishers SET suspended=0 WHERE handle='alice'",
            ),
            (
                "UPDATE registry_accounts SET suspended=1 WHERE id='alice'",
                "UPDATE registry_accounts SET suspended=0 WHERE id='alice'",
            ),
        ]:
            self.db.execute(suspend)
            with self.assertRaises(sqlite3.IntegrityError):
                self.reserve(version="2.0.0")
            with self.assertRaises(sqlite3.IntegrityError):
                self.db.execute("UPDATE versions SET ready=1")
            self.db.execute(restore)
        self.db.execute("UPDATE versions SET quarantined=1")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("UPDATE versions SET ready=1")

    def test_ready_state_is_permanent_but_quarantine_is_reversible(self) -> None:
        self.reserve()
        self.db.execute("UPDATE versions SET ready=1")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("UPDATE versions SET ready=0")
        self.db.execute("UPDATE versions SET quarantined=1")
        self.db.execute("UPDATE versions SET quarantined=0")
        self.assertEqual(self.db.execute("SELECT ready FROM versions").fetchone(), (1,))

    def test_reserved_bytes_and_version_count_cannot_exceed_quota(self) -> None:
        for number in range(32):
            self.reserve(version=f"1.0.{number}", size=8 * 1024 * 1024)
        with self.assertRaises(sqlite3.IntegrityError):
            self.reserve(version="2.0.0", size=1)
        self.db.close()
        self.setUp()
        for number in range(1000):
            self.reserve(version=f"1.0.{number}", size=1)
        with self.assertRaises(sqlite3.IntegrityError):
            self.reserve(version="2.0.0", size=1)

    def test_package_quota_allows_versions_of_existing_packages(self) -> None:
        for number in range(100):
            self.reserve(name=f"package-{number}")
        with self.assertRaises(sqlite3.IntegrityError):
            self.reserve(name="one-too-many")
        self.reserve(name="package-0", version="2.0.0")

    def test_global_storage_cap_cannot_be_bypassed_with_another_account(self) -> None:
        self.db.execute("UPDATE registry_limits SET reserved_bytes=256")
        self.reserve(size=128)
        self.db.execute(
            "INSERT INTO publishers(handle,user_id,display_name) VALUES ('bob','bob','Bob')"
        )
        self.db.execute(
            "INSERT INTO versions(publisher,name,version,manifest_json,description,sha256,size) "
            "VALUES ('bob','test','1.0.0','{}','Fixture',?,128)",
            ("b" * 64,),
        )
        with self.assertRaises(sqlite3.IntegrityError):
            self.reserve(version="2.0.0", size=1)

    def test_global_version_cap_is_independent_of_reserved_bytes(self) -> None:
        self.db.execute("UPDATE registry_limits SET versions=1")
        self.reserve(size=1)
        with self.assertRaises(sqlite3.IntegrityError):
            self.reserve(version="2.0.0", size=1)

    def test_global_limits_are_positive_and_cannot_be_removed(self) -> None:
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("DELETE FROM registry_limits")
        for statement in [
            "UPDATE registry_limits SET reserved_bytes=0",
            "UPDATE registry_limits SET versions=0",
            "UPDATE registry_limits SET accounts=0",
            "UPDATE registry_limits SET publishers=0",
            "UPDATE registry_limits SET daily_upload_bytes=0",
            "UPDATE registry_limits SET daily_upload_count=0",
        ]:
            with self.subTest(statement=statement), self.assertRaises(sqlite3.IntegrityError):
                self.db.execute(statement)

    def test_global_account_and_publisher_caps_allow_existing_identity_retry(self) -> None:
        self.db.execute("UPDATE registry_limits SET accounts=3,publishers=1")
        self.db.execute("INSERT INTO registry_accounts(id) VALUES ('alice') ON CONFLICT DO NOTHING")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("INSERT INTO registry_accounts(id) VALUES ('another')")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute(
                "INSERT INTO publishers(handle,user_id,display_name) VALUES ('bob','bob','Bob')"
            )

    def test_moderation_update_and_audit_roll_back_together(self) -> None:
        self.db.execute("UPDATE registry_accounts SET suspended=1 WHERE id='operator'")
        self.db.commit()
        with self.assertRaises(sqlite3.IntegrityError), self.db:
            self.db.execute("UPDATE publishers SET suspended=1 WHERE handle='alice'")
            self.db.execute(
                "INSERT INTO moderation_log(operator_id,target_type,target,disabled,reason) "
                "VALUES ('operator','publishers','alice',1,'Fixture')"
            )
        self.assertEqual(self.db.execute("SELECT suspended FROM publishers").fetchone(), (0,))
        self.assertEqual(self.db.execute("SELECT count(*) FROM moderation_log").fetchone(), (0,))


if __name__ == "__main__":
    unittest.main()
