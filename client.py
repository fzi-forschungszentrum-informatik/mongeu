import requests

class Client:
    """Mongeu API client"""
    def __init__(self, base_url: str):
        self.base_url = base_url

    def ping(self) -> bool:
        """Ping the mongeu API"""
        return requests.get(self.base_url + "/v1/ping").ok

    def health(self) -> dict:
        r = requests.get(self.base_url + "/v1/health")
        if not r.ok:
            print(r, file=sys.stderr)
            return None
        return r.json()

    def oneshot(self, duration_ms: int) -> dict:
        """Perform a oneshot measurement"""
        if duration_ms is None:
            params = None
        else:
            params = {'duration': duration_ms}

        r = requests.get(self.base_url + "/v1/energy", params=params)
        if not r.ok:
            print(r, file=sys.stderr)
            return None
        return r.json()

    def new_campaign(self):
        """Create a new campaign"""
        r = requests.post(self.base_url + "/v1/energy", allow_redirects=False)
        if not r.is_redirect:
            print(r, file=sys.stderr)
            return None

        location = r.headers['location']
        if location.startswith('http'):
            return Campaign(location)
        return Campaign(self.base_url + location)

    def new_campaign2(self):
        """Create a new campaign"""
        r = requests.post(self.base_url + "/v1/energy", allow_redirects=True)
        if not r.ok:
            print(r, file=sys.stderr)
            return None
        return Campaign(r.url)

class Campaign:
    """A measurement campaign"""
    def __init__(self, url):
        self.url = url

    def get(self) -> dict:
        """Get a current measurement for this campaign"""
        r = requests.get(self.url)
        if not r.ok:
            print(r, file=sys.stderr)
            return None
        return r.json()

    def __del__(self):
        requests.delete(self.url)
