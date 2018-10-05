# coding: utf-8

import os
import json
import time
try:
  import ConfigParser
except:
  import configparser as ConfigParser

from define import Defines as define

class DataManager():
  def __init__(self, collect_dir=None, d2mz_dir=None, opt=None):
    if collect_dir == None:
      self.collect_dir = os.getcwd() + '/'
    else:
      self.collect_dir = collect_dir
    if d2mz_dir == None:
      self.d2mz_dir = "{}/{}/".format(os.getcwd(), define.CONF_ROOT)
    else:
      self.d2mz_dir = d2mz_dir

    # confgi を取得
    self.conf = ConfigParser.ConfigParser()
    self.conf.read("{}/{}".format(self.d2mz_dir, define.CONF_FILE))
    # db キャッシュ作成
    json_f = open("{}/{}".format(self.d2mz_dir, define.CONF_DB), 'r')
    self.db = json.load(json_f)
  def get_d2mz_path(self):
    return self.d2mz_dir
  def get_store_path(self):
    return "{}/{}".format(self.d2mz_dir, define.CONF_STORE)
  def get_collect_path(self):
    return self.collect_dir
  def get_conf_val(self, section, key):
    return self.conf.get(section, key)
  def get_list(self):
    return self.db['files']
  def sync_db(self):
    json_f = open("{}/{}".format(self.d2mz_dir, define.CONF_DB), 'w')
    json.dump(self.db, json_f)
  def insert_or_update_file(self, sha1, name, tags, meta, opt=None):
    data = {
      "name": name,
      "tags":  tags,
      "meta": meta,
      "update_at": time.time()
    }
    #print(data)
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


