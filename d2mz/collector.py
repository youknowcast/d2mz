# coding=utf-8

import os
import re
import hashlib

class Collector():
  def __init__(self, datamgr, target_dir=None):
    self.datamgr = datamgr
    if target_dir == None:
      self.target_dir = os.getcwd()
    else:
      self.target_dir = target_dir

#  def main(self):
    # get data from current dir

  def collect(self):
    current = self.target_dir
    print('collect from current path({})'.format(current))

    targets = os.listdir(current)
    def list_filter(list):
      allowed = self.datamgr.get_conf_val('d2mz', 'file_type')
      def _filter(x):
        r,ext = os.path.splitext(x)
        #print(ext)
        return ext in allowed
      return filter(_filter, list)
    targets = list_filter(targets)
    #print(list(targets))

    for t in targets:
      tmp_path = current + '/' + t
      # hash
      sha1 = self.calc_sha1(tmp_path)
      name, meta = self.get_meta_and_name(t)
      print(name)
      print(meta)
      tags = []
      self.datamgr.insert_or_update_file(sha1, name, tags, meta)
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
  def sub_tag_str(self, txt):
    return txt.translate(str.maketrans({
        '[': '',
        ']': ''
      }))


if __name__ == '__main__':
#  main()
  from datamanager import DataManager
  collector = Collector(DataManager('./'), os.getcwd() + '/tmp/')
  collector.collect()
