import pytest

from tests.client import ApiClient

ADDRESS = "11117Jv1oQo1qkxrKrHXumDZu183yoPRhRXJgqy2D3Gh53bUUZYqY"


@pytest.fixture
def client() -> ApiClient:
    return ApiClient("http://[::1]:8080/api")
