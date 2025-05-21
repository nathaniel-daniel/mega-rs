import io

class File:
    public_id: str | None
    node_id: str | None
    name: str
    key: str

class FileDownload(io.RawIOBase): ...

class FolderEntry:
    id: str
    name: str
    type: str
    key: str

class Client:
    def __init__(self) -> None: ...
    def get_file(self, url=None, node_id=None, key=None) -> File: ...
    def download_file(self, file: File) -> FileDownload: ...
    def list_folder(self, url: str, recursive: bool = False): ...
