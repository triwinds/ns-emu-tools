import ipaddress
import logging
import os
import sys

import dns.message
import dns.query
import dns.rdatatype
import dns.resolver
import requests
from urllib3.util import connection

_orig_create_connection = connection.create_connection
PY3 = sys.version_info >= (3, 0)
logger = logging.getLogger(__name__)

# DNSPod https://doh.pub
# DOH_SERVER = '120.53.53.53'
# aliyun https://dns.alidns.com
DOH_SERVER = '223.5.5.5'

resolver = dns.resolver.Resolver(configure=False)
resolver.nameservers = ["223.5.5.5", '119.29.29.29']

doh_cache = {}


def is_ip_address(hostname: str):
    try:
        ipaddress.ip_address(hostname)
        return True
    except:
        return False


def query_address(name, record_type='A', server=DOH_SERVER, path="/dns-query", fallback=True, verbose=False):
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

    retval = doh_cache.get(name)
    if retval is not None:
        return retval

    try:
        with requests.sessions.Session() as session:
            q = dns.message.make_query(name, dns.rdatatype.from_text(record_type))
            resp = dns.query.https(q, server, session=session)
            # print(f'[{name}] doh answer: {resp.answer}')
            logger.debug(f'doh answer: {resp.answer}')
            if not resp.answer:
                doh_cache[name] = retval
                return []
            retval = []
            for answer in resp.answer:
                for item in answer:
                    retval.append(item.address)
    except Exception as ex:
        if verbose:
            logger.debug("Exception occurred: '%s'" % ex)

    if retval is None and fallback:
        answer = resolver.resolve(name, dns.rdatatype.from_text(record_type))
        logger.debug(f'dns resolver answer: {answer}')
        retval = []
        for item in answer:
            retval.append(item.address)

    if not PY3 and retval:
        retval = [_.encode() for _ in retval]

    doh_cache[name] = retval
    return retval


def patched_create_connection(address, *args, **kwargs):
    """Wrap urllib3's create_connection to resolve the name elsewhere"""
    # resolve hostname to an ip address; use your own
    # resolver here, as otherwise the system resolver will be used.
    host, port = address
    if host.strip() == DOH_SERVER:
        return _orig_create_connection((DOH_SERVER, port), *args, **kwargs)
    addresses = query_address(host)
    if not addresses:
        return _orig_create_connection(address, *args, **kwargs)
    hostname = addresses[0]
    return _orig_create_connection((hostname, port), *args, **kwargs)


def install_doh():
    connection.create_connection = patched_create_connection
    # os.environ['NO_PROXY'] = DOH_SERVER


if __name__ == '__main__':
    # print(query_address('google.com'))
    install_doh()
    print(requests.get('http://t.tt'))
