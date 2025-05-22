import io

class Node:
    public_id: str | None
    id: str | None
    name: str

    def as_dict(self) -> dict[str, object]: ...
    @staticmethod
    def from_dict(value: dict[str, object]) -> "Node": ...

class FileDownload(io.RawIOBase): ...

class FolderEntry:
    id: str
    name: str
    type: str
    key: str

class Client:
    def __init__(self) -> None: ...
    def get_node_from_url(self, url: str) -> Node: ...
    def list_files(self, node: Node, recursive: bool = False) -> list[FolderEntry]: ...
    def download_file(self, file: Node) -> FileDownload: ...
    def get_file(
        self, url: str | None = None, node_id: str | None = None, key: str | None = None
    ) -> Node: ...
