import shutil
import os
import re

class Renamer():
  def __init__(self, datamgr):
    self.datamgr = datamgr
  def rename(self):
    current = self.datamgr.get_collect_path()
    print('rename with current path({})'.format(current))

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

    for t in targets:
      src_path = current + '/' + t
      dst_path = current + '/' + self.create_file_name(t)
 
  def create_file_name(self, filename_org):
    filename = ""
    author = ""
    # ignore ()
    pattern = u"[(][^)]+[)]"
    if (re.match(pattern, filename_org) != None):
      filename = re.sub(pattern, '', filename_org, 1)
    else:
      filename = filename_org
    # 行頭が []
    pattern = u"\[[^]]+\]"
    if (re.match(pattern, filename_org) != None): 
      author = re.match(pattern, filename_org).group()
      filename = re.sub(pattern, '', filename_org)
    else:
      author = "[]"
    #print("{}{}".format(author, filename))
    return "{}{}".format(author, filename)
 
  def move_file(self, src, dst):
    shutil.copy(src, dst)

if __name__ == '__main__':
  from datamanager import DataManager
  renamer = Renamer(DataManager("./tmp"))
  renamer.rename()
