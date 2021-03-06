use std::io::{self, Write};
use std::path::Path;
use std::mem::size_of;
use std::cmp::min;

use std::vec::Vec;

use util::SliceExt;
use mbr::MasterBootRecord;
use vfat::{Shared, Cluster, File, Dir, Entry, FatEntry, Error, Status};
use vfat::{BiosParameterBlock, CachedDevice, Partition};
use traits::{FileSystem, BlockDevice};

#[derive(Debug)]
pub struct VFat {
    device: CachedDevice,
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    sectors_per_fat: u32,
    fat_start_sector: u64,
    data_start_sector: u64,
    pub root_dir_cluster: Cluster,
}

impl VFat {
    pub fn from<T>(mut device: T) -> Result<Shared<Self>, Error>
        where T: BlockDevice + 'static
    {
        let mbr = MasterBootRecord::from(&mut device)?;
        let start = mbr.table[0].relative_sector as u64;
        let bpb = BiosParameterBlock::from(&mut device, start)?;

        //println!("{:#?}", mbr);
        //println!("{:#?}", bpb);

        let BiosParameterBlock {
            bytes_per_sector,
            sectors_per_cluster,
            sectors_per_fat,
            num_reserved_sectors,
            root_dir_cluster,

            num_of_fats,
            ..
        } = bpb;

        let fat_start_sector = start + num_reserved_sectors as u64;
        let data_start_sector = fat_start_sector + num_of_fats as u64 * sectors_per_fat as u64;

        Ok(Shared::new(Self {
            bytes_per_sector,
            sectors_per_cluster,
            sectors_per_fat,
            fat_start_sector,
            data_start_sector,
            root_dir_cluster: Cluster::from(root_dir_cluster),
            device: CachedDevice::new(device, Partition {
                start,
                sector_size: bytes_per_sector as u64,
            }),
        }))
    }

    pub fn read_root_dir_cluster(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let cluster = self.root_dir_cluster;
        self.read_cluster(cluster, 0, buf)
    }

    fn sector(&self, cluster: Cluster) -> u64 {
        self.data_start_sector + cluster.sector(self.sectors_per_cluster)
    }

    /// A method to read from an offset of a cluster into a buffer.
    pub fn read_cluster(&mut self, mut cluster: Cluster, mut offset: usize, mut buf: &mut [u8]) -> io::Result<usize> {
        use vfat::Status::*;

        let bytes_per_sector = self.bytes_per_sector as usize;

        let mut tmp = Vec::with_capacity(bytes_per_sector);
        let len = buf.len();

        'end:
        loop {
            let sector = self.sector(cluster);
            for i in 0..self.sectors_per_cluster as u64 {
                if offset >= bytes_per_sector {
                    offset -= bytes_per_sector;
                    continue;
                }

                self.device.read_all_sector(sector + i, &mut tmp)?;
                {
                    if offset >= tmp.len() {
                        break 'end;
                    }
                    if buf.write(&tmp[offset..])? == 0 {
                        break 'end;
                    }
                }
                tmp.clear();
                offset = 0;
            }

            match self.fat_entry(cluster)?.status() {
                Data(next) => cluster = next,
                Eoc(_) => break,
                _ => break,
            }
        }
        Ok(len - buf.len())
    }

    /// A method to read all of the clusters chained from a starting cluster
    /// into a vector.
    pub fn read_chain(&mut self, start: Cluster, buf: &mut Vec<u8>) -> io::Result<usize> {
        use vfat::Status::*;
        let mut cluster = start;
        loop {
            let sector = self.sector(cluster);
            for i in 0..self.sectors_per_cluster as u64 {
                self.device.read_all_sector(sector + i, buf)?;
            }
            match self.fat_entry(cluster)?.status() {
                Data(next) => cluster = next,
                Eoc(_) => break,
                _ => break,
            }
        }
        Ok(buf.len())
    }

    pub fn sync_chain(&mut self, start: Cluster, remove: bool) -> io::Result<()> {
        use vfat::Status::*;
        let mut cluster = start;
        loop {
            let sector = self.sector(cluster);
            for i in 0..self.sectors_per_cluster as u64 {
                self.device.sync_sector(sector + i, remove)?;
            }
            match self.fat_entry(cluster)?.status() {
                Data(next) => cluster = next,
                Eoc(_) => break,
                _ => break,
            }
        }
        Ok(())
    }

    /// A method to return a reference to a `FatEntry` for a cluster where the
    /// reference points directly into a cached sector.
    fn fat_entry(&mut self, cluster: Cluster) -> io::Result<&FatEntry> {
        let sector_size = self.bytes_per_sector as u64;
        let offset = cluster.fat_offset();
        let sector = self.fat_start_sector + (offset / sector_size);
        let entry = self.device.get(sector)?;
        let entry = &entry[(offset % sector_size) as usize];
        Ok(unsafe { &*(entry as *const u8 as *const FatEntry) })
    }
}

impl<'a> FileSystem for &'a Shared<VFat> {
    type File = File;
    type Dir = Dir;
    type Entry = Entry;

    fn open<P: AsRef<Path>>(self, path: P) -> io::Result<Self::Entry> {
        use std::path::Component;

        let mut root = Dir::root(self.clone());
        for c in path.as_ref().components() {
            match c {
                Component::Normal(p) => {
                    match root.find(p)? {
                        file @ Entry::File(_) => return Ok(file),
                        Entry::Dir(dir) => root = dir,
                    }
                }
                _ => (),
                /*
                Component::CurDir => (),
                Component::ParentDir => unimplemented!(),
                Component::Prefix(_) => return Err(
                    io::Error::new(io::ErrorKind::InvalidInput, "prefix don't support")),
                    */
            }
        }
        Ok(Entry::Dir(root))
        //unimplemented!("path")
    }

    fn create_file<P: AsRef<Path>>(self, _path: P) -> io::Result<Self::File> {
        unimplemented!("read only file system")
    }

    fn create_dir<P>(self, _path: P, _parents: bool) -> io::Result<Self::Dir>
        where P: AsRef<Path>
    {
        unimplemented!("read only file system")
    }

    fn rename<P, Q>(self, _from: P, _to: Q) -> io::Result<()>
        where P: AsRef<Path>, Q: AsRef<Path>
    {
        unimplemented!("read only file system")
    }

    fn remove<P: AsRef<Path>>(self, _path: P, _children: bool) -> io::Result<()> {
        unimplemented!("read only file system")
    }
}
