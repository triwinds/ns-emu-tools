import ipaddress
import logging
import socket
import sys
import time
from typing import Dict, List
from config import config

import dns.message
import dns.query
import dns.rdatatype
import dns.resolver
import requests
from urllib3.util import connection
from urllib.parse import urlparse
from gevent.lock import RLock
import httpx

_orig_create_connection = connection.create_connection
PY3 = sys.version_info >= (3, 0)
logger = logging.getLogger(__name__)

# DNSPod https://doh.pub
# DOH_SERVER = '120.53.53.53'
# aliyun https://dns.alidns.com
DOH_SERVER = '223.5.5.5'
# iQDNS https://iqiq.io/servers.html
# DOH_SERVER = 'https://cn-east.iqiqzz.com/dns-query'
doh_server_name = urlparse(DOH_SERVER).netloc or DOH_SERVER
session = httpx.Client()
query_lock = RLock()

resolver = dns.resolver.Resolver(configure=False)
resolver.nameservers = ["223.5.5.5", '119.29.29.29']


try_ipv6 = connection.HAS_IPV6 and not config.setting.download.disableAria2Ipv6


class DnsCacheItem:
    expire_at: float = 0
    answer = None

    def __repr__(self):
        return f'DnsCacheItem(expire_at={self.expire_at}, answer={self.answer})'

    def __str__(self):
        return self.__repr__()


dns_cache: Dict[str, List[DnsCacheItem]] = {}


def is_ip_address(hostname: str):
    try:
        ipaddress.ip_address(hostname)
        return True
    except:
        return False


def update_dns_cache(name: str, answer):
    item = DnsCacheItem()
    item.expire_at = time.time() + answer.ttl
    item.answer = answer
    available_items = _get_available_items(name)
    available_items.append(item)
    # logger.debug(f'update dns cache [{name}]: {available_items}')
    dns_cache[name] = available_items


def _get_available_items(name: str, record_type: str = 'A'):
    rdtype = dns.rdatatype.from_text(record_type)
    now = time.time()
    cached_items = dns_cache.get(name, [])
    available_items = [item for item in cached_items if item.expire_at > now and item.answer.rdtype == rdtype]
    return available_items


def take_from_dns_cache(name: str, record_type: str = 'A'):
    res = []
    available_items = _get_available_items(name, record_type)
    available_answers = [item.answer for item in available_items]
    for answer in available_answers:
        for ip in answer:
            res.append(ip.address)
    return res


def query_address(name, record_type='A', server=DOH_SERVER, path="/dns-query", fallback=True, verbose=True):
    query_lock.acquire()
    try:
        return _query_address(name, record_type, server, path, fallback, verbose)
    finally:
        query_lock.release()


def _query_address(name, record_type='A', server=DOH_SERVER, path="/dns-query", fallback=True, verbose=True):
    """
    Returns domain name query results retrieved by using DNS over HTTPS protocol

    # Reference: https://developers.cloudflare.com/1.1.1.1/dns-over-https/json-format/

    >>> query_address("one.one.one.one", fallback=False)
    ['1.0.0.1', '1.1.1.1']
    >>> query_address("one", "NS")
    ['a.nic.one.', 'b.nic.one.', 'c.nic.one.', 'd.nic.one.']
    """
    if is_ip_address(name):
        return [name]

    retval = take_from_dns_cache(name, record_type)
    if retval:
        logger.debug(f'use dns answer from cache: {retval}')
        return retval

    try:
        q = dns.message.make_query(name, dns.rdatatype.from_text(record_type))
        resp = dns.query.https(q, server, session=session)
        # print(f'[{name}] doh answer: {resp.answer}')
        logger.debug(f'doh answer of [{name} in {record_type}]: {resp.answer}')
        if not resp.answer:
            return []
        retval = []
        for answer in resp.answer:
            if answer.rdtype not in {dns.rdatatype.AAAA, dns.rdatatype.A}:
                continue
            update_dns_cache(name, answer)
            for item in answer:
                retval.append(item.address)
    except Exception as ex:
        if verbose:
            logger.debug("Exception occurred: '%s'" % ex)

    if not retval and fallback:
        answer: dns.resolver.Answer = resolver.resolve(name, dns.rdatatype.from_text(record_type))
        update_dns_cache(name, answer)
        logger.debug(f'dns resolver answer: {answer.rrset}')
        retval = []
        for item in answer:
            retval.append(item.address)

    if not PY3 and retval:
        retval = [_.encode() for _ in retval]
    return retval


def _try_connect(addresses, port, *args, **kwargs):
    global try_ipv6
    for ip in addresses:
        try:
            sock: socket.socket = _orig_create_connection((ip, port), *args, **kwargs)
            # logger.debug(f'connected to {sock.getpeername()}')
            return sock
        except:
            pass


def patched_create_connection(address, *args, **kwargs):
    """Wrap urllib3's create_connection to resolve the name elsewhere"""
    # resolve hostname to an ip address; use your own
    # resolver here, as otherwise the system resolver will be used.
    global try_ipv6
    host, port = address
    if host.strip() == doh_server_name:
        return _orig_create_connection((doh_server_name, port), *args, **kwargs)
    if try_ipv6:
        addresses = query_address(host, 'AAAA')
        sock = _try_connect(addresses, port, *args, **kwargs)
        if sock:
            return sock
        elif addresses:
            logger.debug(f'IPv6 disabled in DoH.')
            try_ipv6 = False
    addresses = query_address(host)
    sock = _try_connect(addresses, port, *args, **kwargs)
    if sock:
        return sock
    return _orig_create_connection(address, *args, **kwargs)


def install_doh():
    connection.create_connection = patched_create_connection
    # os.environ['NO_PROXY'] = DOH_SERVER


if __name__ == '__main__':
    print(query_address('google.com'))
    print(query_address('archive.org'))
    # print(query_address('google.com'))
    # time.sleep(60)
    # print(query_address('google.com'))
    # install_doh()
    # print(requests.get('https://nsarchive.e6ex.com', timeout=5).text)
    # print(requests.get('https://cfrp.e6ex.com', timeout=5).text)
