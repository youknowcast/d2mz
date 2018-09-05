# conding: utf-8

import os
import json

from define import Defines as define

class Initializer():
  def __init__(self, current=None):
    if current == None:
      self.current = "./"
    else:
      self.current = current

  def main(self):
    d2mz_root = self.current + define.CONF_ROOT
    print('create ./d2mz directory(target: {})..'.format(d2mz_root))
    self.create_dir( d2mz_root )
    self.create_dir( d2mz_root + define.CONF_STORE )
    print('create ./d2mz/store')
    self.create_file( d2mz_root + define.CONF_FILE, self.create_conf )
    print('create ./d2mz/d2mz.conf')
    self.create_file( d2mz_root + define.CONF_DB, self.create_db )
    print('create ./d2mz/db')
    print('...done')

  def create_dir(self, path):
    if os.path.exists( path ):
      print('Dir: {} is already exit. skipped.'.format(path))
      return
    os.mkdir(path)

  def create_file(self, path, creator):
    if os.path.exists( path ):
      print('File: {} is already exit. skipped.'.format(path))
      return
    with open( path, 'w') as f:
      f.write( creator() )

  def create_conf(self):
    return '''# d2mz conf
[d2mz]
local_max_files = {}
max_file_size = {}

    '''.format(define.CONF_LOCAL_MAX_FILES,
               define.CONF_MAX_FILE_SIZE)

  def create_db(self):
    db = {
      'files': {}
    }
    return json.dumps(db)



if __name__ == "__main__":
  ini = Initializer()
  ini.main()

