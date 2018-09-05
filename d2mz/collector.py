# coding=utf-8

import os
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
      allowed = ['.txt', '.png']
      def _filter(x):
        r,ext = os.path.splitext(x)
        return ext in allowed
      return filter(_filter, list)
    targets = list_filter(targets)
    #print(targets)

    for t in targets:
      tmp_path = current + '/' + t
      # hash
      sha1 = self.calc_sha1(tmp_path)
      name = 'test'
      tags = []
      meta = {}
      self.datamgr.insert_or_update_file(sha1, name, tags, meta)
    self.datamgr.sync_db()
    print('done')

  def calc_sha1(self, path):
    sha1 = hashlib.sha1()
    with open(path, 'rb') as f:
      for chunk in iter(lambda: f.read(10 * sha1.block_size), b''):
        sha1.update(chunk)
    return sha1.hexdigest()
#  def collect_one():

if __name__ == '__main__':
#  main()
  from datamanager import DataManager
  collector = Collector(DataManager('./'), os.getcwd() + '/tmp/')
  collector.collect()
