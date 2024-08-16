#!/bin/env python
# Copyright (c) 2024 FZI Forschungszentrum Informatik
# SPDX-License-Identifier: Apache-2.0
import argparse
import requests
import sys
import time

class MongeuClient:
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
        return Measurement.from_json(r.json())

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

    def get(self):
        """Get a current measurement for this campaign"""
        r = requests.get(self.url)
        if not r.ok:
            print(r, file=sys.stderr)
            return None
        return Measurement.from_json(r.json())

    def __del__(self):
        requests.delete(self.url)

class Measurement:
    """A single measurement"""
    def __init__(self, duration: int, devices: list):
        self.duration = duration
        self.devices = devices

    def from_json(data):
        """Create a measurement from a JSON representation"""
        devices = list(map(lambda d: DeviceMeasurement(d['id'], d['energy']), data['devices']))
        return Measurement(data['duration'], devices)

class DeviceMeasurement:
    """Measurement data for a specific device"""
    def __init__(self, id: int, energy: int):
        self.id = id
        self.energy = energy

if __name__ == '__main__':
    def print_measurement(measurement: Measurement):
        line = f"{measurement.duration:>7}ms | "
        line = line + ", ".join(map(lambda d: f"{d.id:>2}: {d.energy:>10}mJ", measurement.devices))
        print(line)

    parser = argparse.ArgumentParser(description='Mongeu API client demo')
    parser.add_argument('url')
    parser.add_argument('action', choices=['ping', 'health', 'oneshot', 'campaign'])
    parser.add_argument('--campaign_method', choices=['1','2'], default='1')
    parser.add_argument('-i', '--interval', type=int, default=500)
    parser.add_argument('-c', '--count', type=int, default=4)
    args = parser.parse_args()

    client = MongeuClient(args.url)

    if args.action == 'ping':
        if not client.ping():
            sys.exit("Could not ping API")

    elif args.action == 'health':
        print(client.health())

    elif args.action == 'oneshot':
        measurement = client.oneshot(args.interval)
        if measurement is None:
            sys.exit("Could not issue oneshot measurement")
        print_measurement(measurement)

    elif args.action == 'campaign':
        if args.campaign_method == 1:
            campaign = client.new_campaign()
        else:
            campaign = client.new_campaign2()

        for _ in range(0,args.count):
            time.sleep(args.interval/1000.0)
            print_measurement(campaign.get())
        del campaign

    else:
        sys.exit("Unknown action: " + args.action)
