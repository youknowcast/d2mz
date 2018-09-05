# coding: utf-8

import os
import json
import time
import ConfigParser

from define import Defines as define

class DataManager():
  def __init__(self, target_dir=None, opt=None):
    if target_dir == None:
      self.target_dir = os.getcwd() + '/'
    else:
      self.target_dir = target_dir
    # confgi を取得
    self.conf = ConfigParser.ConfigParser()
    self.conf.read(self.target_dir + define.CONF_ROOT + define.CONF_FILE)
    # db キャッシュ作成
    json_f = open(self.target_dir + define.CONF_ROOT + define.CONF_DB, 'r')
    self.db = json.load(json_f)
  def get_conf_val(self, section, key):
    return self.conf.get(section, key)
  def get_list(self):
    return self.db['files']
  def sync_db(self):
    json_f = open(self.target_dir + define.CONF_ROOT + define.CONF_DB, 'w')
    json.dump(self.db, json_f)
  def insert_or_update_file(self, sha1, name, tags, meta, opt=None):
    data = {
      "name": name,
      "tags":  tags,
      "meta": meta,
      "update_at": time.time()
    }
    if sha1 in self.db['files'].keys():
      self.db['files'][sha1].update(data)
    else:
      self.db['files'][sha1] = data
  def delete_oldest_file(self):
    if len(self.db['files']) < self.conf.get('d2mz', 'local_max_files'):
      return
    for k,v in self.db['files'].items():
      if oldest == None:
        oldest = k
        oldest_time = v['update_at']
      else:
        if v['update_at'] < oldest_time:
          oldest = k
          oldest_time = v['update_at']
    if oldest != None:
      self.db['files'].pop(oldest)


