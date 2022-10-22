"""
Modify from https://github.com/tanaikech/getfilelistpy/blob/master/getfilelistpy/getfilelist.py


This is a python library to retrieve the file list with the folder tree
from the specific folder of Google Drive.

- This library retrieves all files from a folder in own Google Drive and shared Drives.
- All files include the folder structure in Google Drive.
- Only folder tree can be also retrieved.

usage:
resource = {
    "api_key": api_key,
    # "oauth2": auth,
    # "service_account": credentials,
    "id": "#####",
    "fields": "files(id,name)",
}

res = getfilelist.GetFileList(resource)

res = getfilelist.GetFolderTree(resource)

"""

__author__ = "Kanshi TANAIKE (tanaike@hotmail.com)"
__copyright__ = "Copyright 2018, Kanshi TANAIKE"
__license__ = "MIT"
__version__ = "1.0.5"

# from apiclient.discovery import build
# from apiclient.discovery import build as obuild
from googleapiclient.discovery import build
import collections as cl
import googleapiclient
import sys
import httplib2
import google_auth_httplib2
import urllib


def GetFolderTree(resource):
    return getfilelist(resource).getFolderTree()


def GetFileList(resource):
    return getfilelist(resource).getFileList()


class getfilelist():
    """This is a base class of getfilelistpy."""

    def __init__(self, resource):
        self.id = resource["id"] if "id" in resource.keys() else None
        self.fields = resource["fields"] if "fields" in resource.keys(
        ) else None
        self.service = self.__getService(resource)
        self.e = {}
        self.e["chkAuth"] = self.__checkauth(resource)
        self.__init()

    def __getService(self, resource):
        api = 'drive'
        version = 'v3'
        if "api_key" in resource.keys():
            proxies = urllib.request.getproxies()
            if proxies:
                http = httplib2.Http(proxy_info=httplib2.ProxyInfo(
                    httplib2.socks.PROXY_TYPE_HTTP, '127.0.0.1', 7890
                ))
                return build(api, version, developerKey=resource["api_key"], cache_discovery=False, http=http)
            else:
                return build(api, version, developerKey=resource["api_key"], cache_discovery=False)
        if "service_account" in resource.keys():
            return build(api, version, credentials=resource["service_account"], cache_discovery=False)
        try:
            raise ValueError(
                "Error: You can use API key, OAuth2 and Service account.")
        except ValueError as err:
            print(err)
            sys.exit(1)
        return

    def __getList(self, ptoken, q, fields):
        if "driveId" in self.e["searchedFolder"]:
            driveId = self.e["searchedFolder"].get("driveId")
            return self.service.files().list(q=q, fields=fields, orderBy="name", pageSize=1000, pageToken=ptoken or "", includeItemsFromAllDrives=True, supportsAllDrives=True, corpora="drive", driveId=driveId).execute()
        else:
            return self.service.files().list(q=q, fields=fields, orderBy="name", pageSize=1000, pageToken=ptoken or "", includeItemsFromAllDrives=True, supportsAllDrives=True).execute()

    def __getListLoop(self, q, fields, values):
        nextPageToken = ""
        while True:
            res = self.__getList(nextPageToken, q, fields)
            values.extend(res.get("files"))
            nextPageToken = res.get("nextPageToken")
            if nextPageToken is None:
                break
        return values

    def __getFilesFromFolder(self, folderTree):
        f = cl.OrderedDict()
        f["searchedFolder"] = self.e["searchedFolder"]
        f["folderTree"] = folderTree
        f["fileList"] = []
        if self.fields is None:
            self.fields = "files(createdTime,description,id,mimeType,modifiedTime,name,owners,parents,permissions,shared,size,webContentLink,webViewLink),nextPageToken"
        elif self.fields.find("nextPageToken") == -1:
            self.fields += ",nextPageToken"
        for i, e in enumerate(folderTree["folders"]):
            q = "'%s' in parents and mimeType != 'application/vnd.google-apps.folder' and trashed=false" % e
            fm = self.__getListLoop(q, self.fields, [])
            fe = {"files": []}
            fe["folderTree"] = folderTree["id"][i]
            fe["files"].extend(fm)
            f["fileList"].append(fe)
        f["totalNumberOfFolders"] = len(f["folderTree"]["folders"])
        f["totalNumberOfFiles"] = sum(len(e["files"]) for e in f["fileList"])
        return f

    def __getDlFoldersS(self, searchFolderName, fr):
        fT = cl.OrderedDict()
        fT["id"] = []
        fT["names"] = []
        fT["folders"] = []
        fT["id"].append([fr["search"]])
        fT["names"].append(searchFolderName)
        fT["folders"].append(fr["search"])
        for e in fr["temp"]:
            for f in e:
                fT["folders"].append(f["id"])
                tmp = []
                tmp.extend(f["tree"])
                tmp.append(f["id"])
                fT["id"].append(tmp)
                fT["names"].append(f["name"])
        return fT

    def __getAllfoldersRecursively(self, idd, parents, folders):
        q = "'%s' in parents and mimeType='application/vnd.google-apps.folder' and trashed=false" % idd
        fields = "files(id,mimeType,name,parents,size),nextPageToken"
        files = self.__getListLoop(q, fields, [])
        temp = []
        p = list(parents)
        p.append(idd)
        for e in files:
            obj = {"name": e.get("name"), "id": e.get(
                "id"), "parent": e.get("parents")[0], "tree": p}
            temp.append(obj)
        if len(temp) > 0:
            folders["temp"].append(temp)
            for e in temp:
                self.__getAllfoldersRecursively(
                    e.get("id"), e.get("tree"), folders)
        return folders

    def __getFolderTreeRecursively(self):
        folderTr = {"search": self.e["searchedFolder"]["id"], "temp": []}
        value = self.__getAllfoldersRecursively(
            self.e["searchedFolder"]["id"], [], folderTr)
        return self.__getDlFoldersS(self.e["searchedFolder"].get("name"), value)

    def __createFolderTreeID(self, fm, idd, parents, fls):
        temp = []
        p = list(parents)
        p.append(idd)
        for e in fm:
            if ("parents" in e) and (len(e["parents"]) > 0) and (e["parents"][0] == idd):
                t = {"name": e["name"], "id": e["id"],
                     "parent": e["parents"][0], "tree": p}
                temp.append(t)
        if len(temp) > 0:
            fls["temp"].append(temp)
            for e in temp:
                self.__createFolderTreeID(fm, e["id"], e["tree"], fls)
        return fls

    def __getFromAllFolders(self):
        q = "mimeType='application/vnd.google-apps.folder' and trashed=false"
        fields = "files(id,mimeType,name,parents,size),nextPageToken"
        files = self.__getListLoop(q, fields, [])
        tr = {"search": self.e["searchedFolder"]["id"], "temp": []}
        value = self.__createFolderTreeID(
            files, self.e["searchedFolder"]["id"], [], tr)
        return self.__getDlFoldersS(self.e["searchedFolder"]["name"], value)

    def __checkauth(self, resource):
        if "oauth2" in resource.keys() or "service_account" in resource.keys():
            return True
        return False

    def __getFileInf(self):
        fields = "createdTime,id,mimeType,modifiedTime,name,owners,parents,shared,webContentLink,webViewLink,driveId"
        return self.service.files().get(fileId=self.id, fields=fields, supportsAllDrives=True).execute()

    def __init(self):
        self.e["rootId"] = self.id is None or self.id.lower() == "root"
        if not self.e["chkAuth"] and self.e["rootId"]:
            try:
                raise ValueError(
                    "Error: All folders in Google Drive cannot be retrieved using API key. Please use OAuth2.")
            except ValueError as err:
                print(err)
                sys.exit(1)
        self.id = "root" if self.e["rootId"] else self.id
        try:
            self.e["searchedFolder"] = self.__getFileInf()
        except googleapiclient.errors.HttpError:
            print("Error: Folder ID of '%s' cannot be retrieved. Please confirm whether the folder ID is existing, or the owner of file is that of account. If you want to retrieve other user's folder, please check whether the folder is shared." % self.id)
            sys.exit(1)
        self.e["method"] = (self.e["chkAuth"] or self.e["rootId"]
                            ) and not self.e["searchedFolder"].get("shared")
        return

    def getFileList(self):
        """This is a method for retrieving file list."""
        folderTree = self.__getFromAllFolders(
        ) if self.e["method"] else self.__getFolderTreeRecursively()
        return self.__getFilesFromFolder(folderTree)

    def getFolderTree(self):
        """This is a method for retrieving folder tree."""
        return self.__getFromAllFolders() if self.e["method"] else self.__getFolderTreeRecursively()
