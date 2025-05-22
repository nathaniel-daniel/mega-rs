import io

class Node:
    public_id: str | None
    id: str | None
    name: str

class FileDownload(io.RawIOBase): ...

class FolderEntry:
    id: str
    name: str
    type: str
    key: str

class Client:
    def __init__(self) -> None: ...
    def get_file(
        self, url: str | None = None, node_id: str | None = None, key: str | None = None
    ) -> Node: ...
    def download_file(self, file: Node) -> FileDownload: ...
    def list_folder(self, url: str, recursive: bool = False) -> list[FolderEntry]: ...
