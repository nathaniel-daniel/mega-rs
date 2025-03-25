def get_chunks(size):
    chunks = {}
    p = pp = 0
    i = 1

    while i <= 8 and p < size - i * 0x20000:
        chunks[p] = i * 0x20000
        pp = p
        p += chunks[p]
        i += 1
    while p < size:
        chunks[p] = 0x100000
        pp = p
        p += chunks[p]
    chunks[pp] = size - pp
    if not chunks[pp]:
        del chunks[pp]
    return chunks


print(get_chunks(1024 * 1024 * 10))
print(get_chunks(985472))