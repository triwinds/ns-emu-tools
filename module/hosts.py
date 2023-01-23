# -*- coding: utf-8 -*-
# Modify from https://github.com/jonhadfield/python-hosts due to encoding of hosts file is missing in original repository
""" This module contains classes:
HostsEntry:
A representation of a hosts file entry, i.e. a line containing an IP address
and name(s), a comment, or a blank line/line separator.

Hosts:
A representation of a hosts file, e.g. /etc/hosts and
c:\\\\windows\\\\system32\\\\drivers\\\\etc\\\\hosts for a linux or MS windows
based machine respectively. Each entry being represented as an instance
of the HostsEntry class.
"""

import sys

from urllib.request import urlopen
import os
import re

import socket


def is_ipv4(entry):
    """
    Check if the string provided is a valid ipv4 address
    :param entry: A string representation of an IP address
    :return: True if valid, False if invalid
    """
    try:
        if socket.inet_aton(entry):
            return True
    except socket.error:
        return False


def is_ipv6(entry):
    """
    Check if the string provided is a valid ipv6 address
    :param entry: A string representation of an IP address
    :return: True if valid, False if invalid
    """
    try:
        if socket.inet_pton(socket.AF_INET6, entry):
            return True
    except socket.error:
        return False


def valid_hostnames(hostname_list):
    """
    Check if the supplied list of strings are valid hostnames
    :param hostname_list: A list of strings
    :return: True if the strings are valid hostnames, False if not
    """
    for entry in hostname_list:
        if len(entry) > 255:
            return False
        allowed = re.compile(r'(?!-)[A-Z\d-]{1,63}(?<!-)$', re.IGNORECASE)
        if not all(allowed.match(x) for x in entry.split(".")):
            return False
    return True


def is_readable(path=None):
    """
    Test if the supplied filesystem path can be read
    :param path: A filesystem path
    :return: True if the path is a file that can be read. Otherwise, False
    """
    if os.path.isfile(path) and os.access(path, os.R_OK):
        return True
    return False


def dedupe_list(seq):
    """
    Utility function to remove duplicates from a list
    :param seq: The sequence (list) to deduplicate
    :return: A list with original duplicates removed
    """
    seen = set()
    return [x for x in seq if not (x in seen or seen.add(x))]


class HostsException(Exception):
    """ Base exception class. All Hosts-specific exceptions should subclass
    this class.
    """
    pass


class UnableToWriteHosts(HostsException):
    """ Raised when a Hosts file cannot be written. """
    pass


class HostsEntryException(Exception):
    """ Base exception class. All HostsEntry-specific exceptions should
    subclass this class.
    """
    pass


class InvalidIPv4Address(HostsEntryException):
    """ Raised when a HostsEntry is defined as type 'ipv4' but with an
    invalid address.
    """
    pass


class InvalidIPv6Address(HostsEntryException):
    """ Raised when a HostsEntry is defined as type 'ipv6' but
    with an invalid address.
    """
    pass


class HostsEntry(object):
    """ An entry in a hosts file. """
    __slots__ = ['entry_type', 'address', 'comment', 'names']

    def __init__(self,
                 entry_type=None,
                 address=None,
                 comment=None,
                 names=None):
        """
        Initialise an instance of a Hosts file entry
        :param entry_type: ipv4 | ipv6 | comment | blank
        :param address: The ipv4 or ipv6 address belonging to the instance
        :param comment: The comment belonging to the instance
        :param names: The names that resolve to the specified address
        :return: None
        """
        if not entry_type or entry_type not in ('ipv4',
                                                'ipv6',
                                                'comment',
                                                'blank'):
            raise Exception('entry_type invalid or not specified')

        if entry_type == 'comment' and not comment:
            raise Exception('entry_type comment supplied without value.')

        if entry_type == 'ipv4':
            if not all((address, names)):
                raise Exception('Address and Name(s) must be specified.')
            if not is_ipv4(address):
                raise InvalidIPv4Address()

        if entry_type == 'ipv6':
            if not all((address, names)):
                raise Exception('Address and Name(s) must be specified.')
            if not is_ipv6(address):
                raise InvalidIPv6Address()

        self.entry_type = entry_type
        self.address = address
        self.comment = comment
        self.names = names

    def is_real_entry(self):
        return self.entry_type in ('ipv4', 'ipv6')

    def __repr__(self):
        return "HostsEntry(entry_type=\'{0}\', address=\'{1}\', " \
               "names={2}, comment=\'{3}\')".format(
                   self.entry_type,
                   self.address,
                   self.names,
                   self.comment
                   )

    def __str__(self):
        if self.entry_type in ('ipv4', 'ipv6'):
            return "TYPE={0}, ADDR={1}, NAMES={2}, COMMENT={3}".format(
                self.entry_type,
                self.address,
                " ".join(self.names),
                self.comment
                )
        elif self.entry_type == 'comment':
            return "TYPE = {0}, COMMENT = {1}".format(self.entry_type, self.comment)
        elif self.entry_type == 'blank':
            return "TYPE = {0}".format(self.entry_type)

    @staticmethod
    def get_entry_type(hosts_entry=None):
        """
        Return the type of entry for the line of hosts file passed
        :param hosts_entry: A line from the hosts file
        :return: 'comment' | 'blank' | 'ipv4' | 'ipv6'
        """
        if hosts_entry and isinstance(hosts_entry, str):
            entry = hosts_entry.strip()
            if not entry or not entry[0] or entry[0] == "\n":
                return 'blank'
            if entry[0] == "#":
                return 'comment'
            entry_chunks = entry.split()
            if is_ipv6(entry_chunks[0]):
                return 'ipv6'
            if is_ipv4(entry_chunks[0]):
                return 'ipv4'

    @staticmethod
    def str_to_hostentry(entry):
        """
        Transform a line from a hosts file into an instance of HostsEntry
        :param entry: A line from the hosts file
        :return: An instance of HostsEntry
        """
        split_line = entry.split('#', 1)
        inline_comment = None
        if len(split_line) == 2:
            inline_comment = split_line[1].strip()
            line_parts = split_line[0].strip().split()
        else:
            line_parts = entry.strip().split()
        if is_ipv4(line_parts[0]) and valid_hostnames(line_parts[1:]):
            return HostsEntry(entry_type='ipv4',
                              address=line_parts[0],
                              names=line_parts[1:],
                              comment=inline_comment)
        elif is_ipv6(line_parts[0]) and valid_hostnames(line_parts[1:]):
            return HostsEntry(entry_type='ipv6',
                              address=line_parts[0],
                              names=line_parts[1:],
                              comment=inline_comment)
        else:
            return False


class Hosts(object):
    """ A hosts file. """
    __slots__ = ['entries', 'hosts_path']

    def __init__(self, path=None):
        """
        Initialise an instance of a hosts file
        :param path: The filesystem path of the hosts file to manage
        :return: None
        """

        self.entries = []
        if path:
            self.hosts_path = path
        else:
            self.hosts_path = self.determine_hosts_path()
        self.populate_entries()

    def __repr__(self):
        return 'Hosts(hosts_path=\'{0}\', entries={1})'.format(self.hosts_path, self.entries)

    def __str__(self):
        output = ('hosts_path={0}, '.format(self.hosts_path))
        for entry in self.entries:
            output += str(entry)
        return output

    def count(self):
        """ Get a count of the number of host entries
        :return: The number of host entries
        """
        return len(self.entries)

    @staticmethod
    def determine_hosts_path(platform=None):
        """
        Return the hosts file path based on the supplied
        or detected platform.
        :param platform: a string used to identify the platform
        :return: detected filesystem path of the hosts file
        """
        if not platform:
            platform = sys.platform
        if platform.startswith('win'):
            result = r"c:\windows\system32\drivers\etc\hosts"
            return result
        else:
            return '/etc/hosts'

    def write(self, path=None, mode='w'):
        """
        Write all of the HostsEntry instances back to the hosts file
        :param path: override the write path
        :return: Dictionary containing counts
        """
        written_count = 0
        comments_written = 0
        blanks_written = 0
        ipv4_entries_written = 0
        ipv6_entries_written = 0
        if path:
            output_file_path = path
        else:
            output_file_path = self.hosts_path
        try:
            with open(output_file_path, mode, encoding='utf-8') as hosts_file:
                for written_count, line in enumerate(self.entries):
                    if line.entry_type == 'comment':
                        hosts_file.write(line.comment + "\n")
                        comments_written += 1
                    if line.entry_type == 'blank':
                        hosts_file.write("\n")
                        blanks_written += 1
                    if line.entry_type == 'ipv4':
                        hosts_file.write(
                            "{0}\t{1}{2}\n".format(
                                line.address,
                                ' '.join(line.names),
                                " # " + line.comment if line.comment else ""
                            )
                        )
                        ipv4_entries_written += 1
                    if line.entry_type == 'ipv6':
                        hosts_file.write(
                            "{0}\t{1}{2}\n".format(
                                line.address,
                                ' '.join(line.names),
                                " # " + line.comment if line.comment else ""
                            )
                        )
                        ipv6_entries_written += 1
        except:
            raise UnableToWriteHosts()
        return {'total_written': written_count + 1,
                'comments_written': comments_written,
                'blanks_written': blanks_written,
                'ipv4_entries_written': ipv4_entries_written,
                'ipv6_entries_written': ipv6_entries_written}

    @staticmethod
    def get_hosts_by_url(url=None):
        """
        Request the content of a URL and return the response
        :param url: The URL of the hosts file to download
        :return: The content of the passed URL
        """
        response = urlopen(url)
        return response.read()

    def exists(self, address=None, names=None, comment=None):
        """
        Determine if the supplied address and/or names, or comment, exists in a HostsEntry within Hosts
        :param address: An ipv4 or ipv6 address to search for
        :param names: A list of names to search for
        :param comment: A comment to search for
        :return: True if a supplied address, name, or comment is found. Otherwise, False.
        """
        for name in (names or [None]):
            if self.find_all_matching(address=address, name=name, comment=comment):
                return True

        for entry in self.entries:
            if entry.entry_type == 'comment' and entry.comment == comment:
                return True
            # elif entry.entry_type in ('ipv4', 'ipv6'):
            #     pass # already covered above
        return False

    def remove_all_matching(self, address=None, name=None, comment=None):
        """
        Remove all HostsEntry instances from the Hosts object
        where the supplied ip address, name or comment matches
        :param address: An ipv4 or ipv6 address
        :param name: A host name
        :param comment: A host inline comment
        :return: None
        """
        if address or name or comment:
            pass
        else:
            raise ValueError('No address, name or comment was specified for removal.')

        result = self.find_all_matching(
            address=address,
            name=name,
            comment=comment
            )
        self.entries = list(filter(lambda x: x not in result, self.entries))

    def find_all_matching(self, address=None, name=None, comment=None):
        """
        Return all HostsEntry instances from the Hosts object
        where the supplied ip address or name matches
        :param address: An ipv4 or ipv6 address
        :param name: A host name
        :param comment: A host inline comment
        :return: HostEntry instances
        """
        results = []
        if address or name or comment:
            for entry in self.entries:
                if not entry.is_real_entry():
                    continue
                if address:
                    if address != entry.address:
                        continue
                if name:
                    if name not in entry.names:
                        continue
                if comment:
                    if comment != entry.comment:
                        continue
                results.append(entry)
        return results

    def import_url(self, url=None, force=None):
        """
        Read a list of host entries from a URL, convert them into instances of HostsEntry and
        then append to the list of entries in Hosts
        :param url: The URL of where to download a hosts file
        :return: Counts reflecting the attempted additions
        """
        file_contents = self.get_hosts_by_url(url=url).decode('utf-8')
        file_contents = file_contents.rstrip().replace('^M', '\n')
        file_contents = file_contents.rstrip().replace('\r\n', '\n')
        lines = file_contents.split('\n')
        skipped = 0
        import_entries = []
        for line in lines:
            stripped_entry = line.strip()
            if (not stripped_entry) or (stripped_entry.startswith('#')):
                skipped += 1
            else:
                line = line.partition('#')[0]
                line = line.rstrip()
                import_entry = HostsEntry.str_to_hostentry(line)
                if import_entry:
                    import_entries.append(import_entry)
        add_result = self.add(entries=import_entries, force=force)
        write_result = self.write()
        return {'result': 'success',
                'skipped': skipped,
                'add_result': add_result,
                'write_result': write_result}

    def import_file(self, import_file_path=None):
        """
        Read a list of host entries from a file, convert them into instances
        of HostsEntry and then append to the list of entries in Hosts
        :param import_file_path: The path to the file containing the host entries
        :return: Counts reflecting the attempted additions
        """
        skipped = 0
        invalid_count = 0
        if is_readable(import_file_path):
            import_entries = []
            with open(import_file_path, 'r', encoding='utf-8') as infile:
                for line in infile:
                    stripped_entry = line.strip()
                    if (not stripped_entry) or (stripped_entry.startswith('#')):
                        skipped += 1
                    else:
                        line = line.partition('#')[0]
                        line = line.rstrip()
                        import_entry = HostsEntry.str_to_hostentry(line)
                        if import_entry:
                            import_entries.append(import_entry)
                        else:
                            invalid_count += 1
            add_result = self.add(entries=import_entries)
            write_result = self.write()
            return {'result': 'success',
                    'skipped': skipped,
                    'invalid_count': invalid_count,
                    'add_result': add_result,
                    'write_result': write_result}
        else:
            return {'result': 'failed',
                    'message': 'Cannot read: file {0}.'.format(import_file_path)}

    def add(self, entries=None, force=False, allow_address_duplication=False, merge_names=False):
        """
        Add instances of HostsEntry to the instance of Hosts.
        :param entries: A list of instances of HostsEntry
        :param force: Remove matching before adding
        :param allow_address_duplication: Allow using multiple entries for same address
        :param merge_names: Merge names where address already exists
        :return: The counts of successes and failures
        """
        ipv4_count = 0
        ipv6_count = 0
        comment_count = 0
        invalid_count = 0
        duplicate_count = 0
        replaced_count = 0
        import_entries = []
        existing_addresses = [x.address for x in self.entries if x.address]
        existing_names = []
        for item in self.entries:
            if item.names:
                existing_names.extend(item.names)
        existing_names = dedupe_list(existing_names)
        for entry in entries:
            if entry.entry_type == 'comment':
                entry.comment = entry.comment.strip()
                if entry.comment[0] != "#":
                    entry.comment = "# " + entry.comment
                import_entries.append(entry)
            elif entry.address in ('0.0.0.0', '127.0.0.1') or allow_address_duplication:
                # Allow duplicates entries for addresses used for adblocking
                if set(entry.names).intersection(existing_names):
                    if force:
                        for name in entry.names:
                            self.remove_all_matching(name=name)
                        import_entries.append(entry)
                    else:
                        duplicate_count += 1
                else:
                    import_entries.append(entry)
            elif entry.address in existing_addresses:
                if not any((force, merge_names)):
                    duplicate_count += 1
                elif merge_names:
                    # get the last entry with matching address
                    entry_names = list()
                    for existing_entry in self.entries:
                        if entry.address == existing_entry.address:
                            entry_names = existing_entry.names
                            break
                    # merge names with that entry
                    merged_names = list(set(entry.names + entry_names))
                    # remove all matching
                    self.remove_all_matching(address=entry.address)
                    # append merged entry
                    entry.names = merged_names
                    import_entries.append(entry)
                elif force:
                    self.remove_all_matching(address=entry.address)
                    replaced_count += 1
                    import_entries.append(entry)
            elif set(entry.names).intersection(existing_names):
                if not force:
                    duplicate_count += 1
                else:
                    for name in entry.names:
                        self.remove_all_matching(name=name)
                    replaced_count += 1
                    import_entries.append(entry)
            else:
                import_entries.append(entry)

        for item in import_entries:
            if item.entry_type == 'comment':
                comment_count += 1
                self.entries.append(item)
            elif item.entry_type == 'ipv4':
                ipv4_count += 1
                self.entries.append(item)
            elif item.entry_type == 'ipv6':
                ipv6_count += 1
                self.entries.append(item)
        return {'comment_count': comment_count,
                'ipv4_count': ipv4_count,
                'ipv6_count': ipv6_count,
                'invalid_count': invalid_count,
                'duplicate_count': duplicate_count,
                'replaced_count': replaced_count}

    def populate_entries(self):
        """
        Called by the initialiser of Hosts. This reads the entries from the local hosts file,
        converts them into instances of HostsEntry and adds them to the Hosts list of entries.
        :return: None
        """
        try:
            with open(self.hosts_path, 'r', encoding='utf-8') as hosts_file:
                hosts_entries = [line for line in hosts_file]
                for hosts_entry in hosts_entries:
                    entry_type = HostsEntry.get_entry_type(hosts_entry)
                    if entry_type == "comment":
                        hosts_entry = hosts_entry.replace("\r", "")
                        hosts_entry = hosts_entry.replace("\n", "")
                        self.entries.append(HostsEntry(entry_type="comment",
                                                       comment=hosts_entry))
                    elif entry_type == "blank":
                        self.entries.append(HostsEntry(entry_type="blank"))
                    elif entry_type in ("ipv4", "ipv6"):
                        split_entry = hosts_entry.split('#', 1)
                        chunked_entry = split_entry[0].split()
                        comment = None
                        if len(split_entry) == 2:
                            comment = split_entry[1].strip()
                        stripped_name_list = [name.strip() for name in chunked_entry[1:]]

                        self.entries.append(
                            HostsEntry(
                                entry_type=entry_type,
                                address=chunked_entry[0].strip(),
                                names=stripped_name_list,
                                comment=comment))
        except IOError:
            return {'result': 'failed',
                    'message': 'Cannot read: {0}.'.format(self.hosts_path)}
