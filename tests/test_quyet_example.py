"""Tests for the optional Quyet HTTP adapter, with a deterministic model fixture."""
import unittest
from fastapi.testclient import TestClient
from smoke.systemone.quyet_server import create_app


class ModelFixture:
    def predict(self, state, questions, strict=False):
        if "invalid" in questions:
            raise ValueError("invalid question")
        return {"model": "Quyet-1.0-Small", "answers": {"urgent": {"type": "noul", "noul": 0.9}}, "usage": {"input_tokens": 8, "output_tokens": 0}, "warnings": []}


class QuyetExampleTests(unittest.TestCase):
    def setUp(self):
        self.model = ModelFixture()
        self.client = TestClient(create_app(self.model, "quyet-small", "backend-test-key"))
        self.body = {"model": "quyet-small", "state": {"message": "lost card"}, "questions": {"urgent": {"type": "noul", "instructions": "Urgent?"}}}
        self.headers = {"authorization": "Bearer backend-test-key"}

    def test_health_and_authenticated_model_discovery(self):
        self.assertEqual(self.client.get("/health").status_code, 200)
        self.assertEqual(self.client.get("/v1/models").status_code, 401)
        data = self.client.get("/v1/models", headers=self.headers).json()
        self.assertEqual(data["data"][0]["id"], "quyet-small")

    def test_both_decision_endpoints_preserve_model_response(self):
        for path in ("/v1/systemone", "/v1/decisions"):
            response = self.client.post(path, json=self.body, headers=self.headers)
            self.assertEqual(response.status_code, 200)
            self.assertEqual(response.json(), self.model.predict(self.body["state"], self.body["questions"]))

    def test_auth_required_and_optional(self):
        self.assertEqual(self.client.post("/v1/systemone", json=self.body).status_code, 401)
        no_auth = TestClient(create_app(self.model, "quyet-small"))
        self.assertEqual(no_auth.post("/v1/systemone", json=self.body).status_code, 200)

    def test_model_and_question_errors_are_explicit(self):
        for changes, expected in (({"model": "wrong-model"}, 404), ({"questions": {}}, 400), ({"state": 123}, 400), ({"questions": {"invalid": {}}}, 400)):
            self.assertEqual(self.client.post("/v1/systemone", json=dict(self.body, **changes), headers=self.headers).status_code, expected)


if __name__ == "__main__":
    unittest.main()
