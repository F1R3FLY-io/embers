import json
from dataclasses import dataclass
from functools import cached_property
from typing import Any

import requests


@dataclass
class Responce:
    status: int
    body: str

    def __init__(self, r: requests.Response):
        self.status = r.status_code
        self.body = r.text

    @cached_property
    def json(self) -> Any:
        return json.loads(self.body)


class HttpClient:
    def __init__(self, base_url: str):
        self._base_url = base_url

    def get(self, url: str) -> Responce:
        url = self._base_url + url
        r = requests.get(url, timeout=10)
        return Responce(r)

    def post(self, url: str, json: Any | None = None) -> Responce:
        url = self._base_url + url
        r = requests.post(url, json=json, timeout=10)
        return Responce(r)


class WalletsApi:
    def __init__(self, client: HttpClient):
        self._client = client

    def get_wallet_state_and_history(self, address: str) -> Responce:
        return self._client.get(f"/wallets/{address}/state")


class ApiClient:
    def __init__(self, backend_url: str):
        self._http_client = HttpClient(backend_url)
        self.wallets = WalletsApi(self._http_client)
