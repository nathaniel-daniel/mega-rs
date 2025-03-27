# mega-rs
A Rust API for mega

## Features
`easy`: Enable the easy interface, which exposes an easier to use, higher level api client

## Python Binding
This repository also contains a small Python binding.
Here's an example of how it may be used to download a file:
```python
import mega_py
import shutil

client = mega_py.Client()
file = client.get_file('https://mega.nz/file/7glwEQBT#Fy9cwPpCmuaVdEkW19qwBLaiMeyufB1kseqisOAxfi8')

with open(file.name, 'wb') as dest:
    shutil.copyfileobj(client.download_file(file), dest)
```

## References
 * http://julien-marchand.fr/blog/using-mega-api-with-python-examples/
 * https://github.com/meganz/sdk/blob/9a951c9db1734cac3f44603f0491bc9755986aa7/doc/source/internals.rst
 * https://mega.io/doc
 * https://github.com/meganz/sdk
