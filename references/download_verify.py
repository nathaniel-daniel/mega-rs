import urllib.request
from Crypto.Cipher import AES
from Crypto.Util import Counter
import struct

dl_url = 'http://gfs204n145.userstorage.mega.co.nz/dl/Y3Fyr2suCDwzKAIUKHV-WlG62CmWEUAid2yobDSHWVIyzvO95yZ0N7GYBKS5v7WRb01PIsIe4_ZhWzbe3ehyTV72U-3TSWT5V4E1eVs_m49FdpZtOPBVyVK3VnGmHQ'
# replaced_key = a32_to_str(k)
replaced_key = bytes([
    161,
    141,
    109,
    44,
    84,
    62,
    135,
    130,
    36,
    158,
    235,
    166,
    55,
    235,
    206,
    43,
])
# replaced_iv = ((iv[0] << 32) + iv[1]) << 64
replaced_iv_bytes = bytes([
    182,
    162,
    49,
    236,
    174,
    124,
    29,
    100,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
])
replaced_iv = int.from_bytes(replaced_iv_bytes, byteorder='big')

replaced_meta_mac = bytes([177, 234, 162, 176, 224, 49, 126, 47])

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
    

attributes = {
    'n': 'out.bin'
}
file = {
    's': 985472,
}

def a32_to_str(a):
  return struct.pack('>%dI' % len(a), *a)
  
def aes_cbc_encrypt(data, key):
  encryptor = AES.new(key, AES.MODE_CBC, bytes([0] * 16))
  return encryptor.encrypt(data)
 
def aes_cbc_encrypt_a32(data, key):
  return str_to_a32(aes_cbc_encrypt(a32_to_str(data), a32_to_str(key)))
  
def str_to_a32(b):
  if len(b) % 4: # Add padding, we need a string with a length multiple of 4
    b += '\0' * (4 - len(b) % 4)
  return struct.unpack('>%dI' % (len(b) / 4), b)
  
  
replaced_meta_mac = str_to_a32(replaced_meta_mac)

infile = urllib.request.urlopen(dl_url)
outfile = open(attributes['n'], 'wb')
decryptor = AES.new(replaced_key, AES.MODE_CTR, counter = Counter.new(128, initial_value = replaced_iv))
 
file_mac = [0, 0, 0, 0]
for chunk_start, chunk_size in sorted(get_chunks(file['s']).items()):
  chunk = infile.read(chunk_size)
  # Decrypt and upload the chunk
  chunk = decryptor.decrypt(chunk)
  outfile.write(chunk)
  
  
  # Compute the chunk's MAC
  # chunk_mac = [iv[0], iv[1], iv[0], iv[1]]
  chunk_mac = str_to_a32(replaced_iv_bytes[:8]) + str_to_a32(replaced_iv_bytes[:8])
  for i in range(0, len(chunk), 16):
    block = chunk[i:i+16]
    if len(block) % 16:
      block += '\0' * (16 - (len(block) % 16))
      raise RuntimeError('Not Possible')
    block = str_to_a32(block)
    print(list(a32_to_str(block)))
    chunk_mac = [chunk_mac[0] ^ block[0], chunk_mac[1] ^ block[1], chunk_mac[2] ^ block[2], chunk_mac[3] ^ block[3]]
    # chunk_mac = aes_cbc_encrypt_a32(chunk_mac, k)
    chunk_mac = aes_cbc_encrypt_a32(chunk_mac, str_to_a32(replaced_key))
 
  # print(list(a32_to_str(chunk_mac)))
  # Update the file's MAC
  file_mac = [file_mac[0] ^ chunk_mac[0], file_mac[1] ^ chunk_mac[1], file_mac[2] ^ chunk_mac[2], file_mac[3] ^ chunk_mac[3]]
  # file_mac = aes_cbc_encrypt_a32(file_mac, k)
  file_mac = aes_cbc_encrypt_a32(file_mac, str_to_a32(replaced_key))
 
outfile.close()
infile.close()
 
# Integrity check
print(a32_to_str(file_mac))
# if (file_mac[0] ^ file_mac[1], file_mac[2] ^ file_mac[3]) != meta_mac:
if (file_mac[0] ^ file_mac[1], file_mac[2] ^ file_mac[3]) != replaced_meta_mac:
  print("MAC mismatch")