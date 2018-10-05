# coding=utf-8

import os
import shutil
import re
import hashlib

from define import Defines as define

class Collector():
  def __init__(self, datamgr):
    self.datamgr = datamgr

  def collect(self):
    current = self.datamgr.get_collect_path()
    print('collect from current path({})'.format(current))

    targets = os.listdir(current)
    def list_filter(list):
      allowed = self.datamgr.get_conf_val('d2mz', 'file_type')
      def _filter(x):
        r,ext = os.path.splitext(x)
        #print(ext)
        if ext == "":
          return False
        return ext in allowed
      return filter(_filter, list)
    targets = list_filter(targets)
    #print(list(targets))

    for t in targets:
      tmp_path = current + '/' + t
      # hash
      sha1 = self.calc_sha1(tmp_path)
      name, meta = self.get_meta_and_name(t)
      tags = []
      self.datamgr.insert_or_update_file(sha1, name, tags, meta)
      ext = os.path.splitext(tmp_path)[1]
      dst_path = "{}/{}{}".format(self.datamgr.get_store_path(),
                                  sha1, ext)
      self.move_file(tmp_path, dst_path)
    self.datamgr.sync_db()
    print('done')

  def calc_sha1(self, path):
    sha1 = hashlib.sha1()
    with open(path, 'rb') as f:
      for chunk in iter(lambda: f.read(10 * sha1.block_size), b''):
        sha1.update(chunk)
    return sha1.hexdigest()

  def get_meta_and_name(self, filename):
    ret = ""
    meta = {}
    #print(filename)
    # ignore ()
    pattern = u"[(][^)]+[)]"
    if (re.match(pattern, filename) != None):
      ret = re.sub(pattern, '', filename, 1)
    else:
      ret = filename
    #print(ret)
    # [] regard as tags
    pattern = u"\[[^]]+\]"
    if (re.match(pattern, ret) != None):
      author = re.match(pattern, ret).group()
      ret = re.sub(pattern, '', ret)
      meta['author'] = self.sub_tag_str(author)
    return ret, meta

  def move_file(self, src, dst):
    print(dst)
    shutil.copy(src, dst)

  def sub_tag_str(self, txt):
    return txt.translate(str.maketrans({
        '[': '',
        ']': ''
      }))


if __name__ == '__main__':
  # for test
  #   tmp/hoge.log
  #   .d2mz/
  #   src
  from datamanager import DataManager
  collector = Collector(DataManager("./tmp"))
  collector.collect()
