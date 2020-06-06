//! Block storage.

use crate::blocktree::Height;

use bitcoin::blockdata::block::BlockHeader;
use bitcoin::consensus::encode;
use nonempty::NonEmpty;
use thiserror::Error;

use std::fmt;

#[derive(Debug, Error)]
pub enum Error {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("error decoding block: {0}")]
    Decoding(#[from] encode::Error),
    #[error("error: the store data is corrupt")]
    Corruption,
}

pub trait Store: fmt::Debug {
    /// Get the genesis block.
    fn genesis(&self) -> Result<BlockHeader, Error> {
        self.get(0)
    }
    /// Append a batch of consecutive block headers to the end of the chain.
    fn put<I: Iterator<Item = BlockHeader>>(&mut self, headers: I) -> Result<Height, Error>;
    /// Get the block at the given height.
    fn get(&self, height: Height) -> Result<BlockHeader, Error>;
    /// Rollback the chain to the given height.
    fn rollback(&mut self, height: Height) -> Result<(), Error>;
    /// Synchronize the changes to disk.
    fn sync(&mut self) -> Result<(), Error>;
    /// Iterate over all headers in the store.
    fn iter(&self) -> Box<dyn Iterator<Item = Result<(Height, BlockHeader), Error>>>;
    /// Return the number of headers in the store.
    fn len(&self) -> Result<usize, Error>;
}

#[derive(Debug, Clone)]
pub struct Memory(NonEmpty<BlockHeader>);

impl Memory {
    pub fn new(chain: NonEmpty<BlockHeader>) -> Self {
        Self(chain)
    }
}

impl Store for Memory {
    /// Get the genesis block.
    fn genesis(&self) -> Result<BlockHeader, Error> {
        Ok(self.0.first().clone())
    }

    /// Append a batch of consecutive block headers to the end of the chain.
    fn put<I: Iterator<Item = BlockHeader>>(&mut self, headers: I) -> Result<Height, Error> {
        self.0.tail.extend(headers);
        Ok(self.0.len() as Height - 1)
    }

    /// Get the block at the given height.
    fn get(&self, height: Height) -> Result<BlockHeader, Error> {
        match self.0.get(height as usize) {
            Some(header) => Ok(header.clone()),
            None => Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "unexpected end of file",
            ))),
        }
    }

    /// Rollback the chain to the given height.
    fn rollback(&mut self, height: Height) -> Result<(), Error> {
        match height {
            0 => self.0.tail.clear(),
            h => self.0.tail.truncate(h as usize + 1),
        }
        Ok(())
    }

    /// Synchronize the changes to disk.
    fn sync(&mut self) -> Result<(), Error> {
        Ok(())
    }

    /// Iterate over all headers in the store.
    fn iter(&self) -> Box<dyn Iterator<Item = Result<(Height, BlockHeader), Error>>> {
        Box::new(
            self.0
                .clone()
                .into_iter()
                .enumerate()
                .map(|(i, h)| Ok((i as Height, h))),
        )
    }

    /// Return the number of headers in the store.
    fn len(&self) -> Result<usize, Error> {
        Ok(self.0.len())
    }
}

pub mod io {
    use super::{Error, Store};
    use crate::blocktree::Height;

    use bitcoin::blockdata::block::BlockHeader;
    use bitcoin::consensus::encode::{Decodable, Encodable};

    use std::fs::{self, File};
    use std::io::{self, Read, Seek, Write};
    use std::iter;
    use std::path::Path;

    // Size in bytes of a block header.
    const HEADER_SIZE: usize = 80;

    /// Append a block to the end of the file.
    fn put<S: Seek + Write, I: Iterator<Item = BlockHeader>>(
        mut stream: S,
        headers: I,
    ) -> Result<Height, Error> {
        let mut pos = stream.seek(io::SeekFrom::End(0))?;

        for header in headers {
            pos += header.consensus_encode(&mut stream)? as u64;
        }
        Ok(pos / HEADER_SIZE as u64 - 1)
    }

    fn get<S: Seek + Read>(mut stream: S, height: Height) -> Result<BlockHeader, Error> {
        let mut buf = [0; HEADER_SIZE];

        stream.seek(io::SeekFrom::Start(height * HEADER_SIZE as u64))?;
        stream.read_exact(&mut buf)?;

        BlockHeader::consensus_decode(&buf[..]).map_err(Error::from)
    }

    /// An iterator over block headers in a file.
    pub struct Iter {
        height: Height,
        file: File,
    }

    impl Iterator for Iter {
        type Item = Result<(Height, BlockHeader), Error>;

        fn next(&mut self) -> Option<Self::Item> {
            let height = self.height;

            match get(&mut self.file, height) {
                // If we hit this branch, it's because we're trying to read passed the end
                // of the file, which means there are no further headers remaining.
                Err(Error::Io(err)) if err.kind() == io::ErrorKind::UnexpectedEof => None,
                // If another kind of error occurs, we want to yield it to the caller, so
                // that it can be propagated.
                Err(err) => Some(Err(err)),
                Ok(header) => {
                    self.height = height + 1;
                    Some(Ok((height, header)))
                }
            }
        }
    }

    /// A `Store` backed by a single file.
    #[derive(Debug)]
    pub struct FileStore {
        file: File,
    }

    impl FileStore {
        pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
            fs::OpenOptions::new()
                .create(true)
                .read(true)
                .append(true)
                .open(path)
                .map(|file| Self { file })
        }

        pub fn create<P: AsRef<Path>>(path: P, genesis: BlockHeader) -> Result<Self, Error> {
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .read(true)
                .append(true)
                .open(path)?;

            put(&mut file, iter::once(genesis))?;

            Ok(Self { file })
        }
    }

    impl Store for FileStore {
        /// Append a block to the end of the file.
        fn put<I: Iterator<Item = BlockHeader>>(&mut self, headers: I) -> Result<Height, Error> {
            self::put(&mut self.file, headers)
        }

        /// Get the block at the given height. Returns `io::ErrorKind::UnexpectedEof` if
        /// the height is not found.
        fn get(&self, height: Height) -> Result<BlockHeader, Error> {
            // Clone so this function doesn't have to take a `&mut self`.
            let mut file = self.file.try_clone()?;

            get(&mut file, height)
        }

        /// Rollback the chain to the given height. Behavior is undefined if  the given
        /// height is not contained in the store.
        fn rollback(&mut self, height: Height) -> Result<(), Error> {
            self.file
                .set_len((height + 1) * HEADER_SIZE as u64)
                .map_err(Error::from)
        }

        /// Flush changes to disk.
        fn sync(&mut self) -> Result<(), Error> {
            self.file.sync_data().map_err(Error::from)
        }

        /// Iterate over all headers in the store.
        fn iter(&self) -> Box<dyn Iterator<Item = Result<(Height, BlockHeader), Error>>> {
            // Clone so this function doesn't have to take a `&mut self`.
            match self.file.try_clone() {
                Ok(file) => Box::new(Iter { height: 0, file }),
                Err(err) => Box::new(iter::once(Err(Error::Io(err)))),
            }
        }

        /// Return the number of headers in the store.
        fn len(&self) -> Result<usize, Error> {
            let meta = self.file.metadata()?;
            let len = meta.len();

            assert!(len <= usize::MAX as u64);

            if len as usize % HEADER_SIZE != 0 {
                return Err(Error::Corruption);
            }
            Ok(len as usize / HEADER_SIZE)
        }
    }

    #[cfg(test)]
    mod test {
        use super::{BlockHeader, FileStore, Height, Store};
        use std::iter;

        #[test]
        fn test_put_get() {
            let tmp = tempfile::tempdir().unwrap();
            let mut store = FileStore::open(tmp.path().join("headers.db")).unwrap();

            let header = BlockHeader {
                version: 1,
                prev_blockhash: Default::default(),
                merkle_root: Default::default(),
                bits: 0x2ffffff,
                time: 1842918273,
                nonce: 312143,
            };

            assert!(
                store.get(0).is_err(),
                "when the store is empty, there is nothing to `get`"
            );

            let height = store.put(iter::once(header)).unwrap();
            store.sync().unwrap();

            assert_eq!(height, 0);
            assert_eq!(store.get(height).unwrap(), header);
            assert!(store.get(1).is_err());
        }

        #[test]
        fn test_put_get_batch() {
            let tmp = tempfile::tempdir().unwrap();
            let mut store = FileStore::open(tmp.path().join("headers.db")).unwrap();

            assert!(
                store.get(0).is_err(),
                "when the store is empty, there is nothing to `get`"
            );
            assert_eq!(store.len().unwrap(), 0);

            let count = 32;
            let header = BlockHeader {
                version: 1,
                prev_blockhash: Default::default(),
                merkle_root: Default::default(),
                bits: 0x2ffffff,
                time: 1842918273,
                nonce: 0,
            };
            let iter = (0..count).map(|i| BlockHeader { nonce: i, ..header });
            let headers = iter.clone().collect::<Vec<_>>();

            // Put all headers into the store and check that we can retrieve them.
            {
                let height = store.put(iter).unwrap();

                assert_eq!(height, headers.len() as Height - 1);
                assert_eq!(store.len().unwrap(), headers.len());

                for (i, h) in headers.iter().enumerate() {
                    assert_eq!(&store.get(i as Height).unwrap(), h);
                }

                assert!(&store.get(32).is_err());
                assert!(&store.get(64).is_err());
            }

            // Rollback and overwrite the history.
            {
                let h = headers.len() as Height / 2; // Some point `h` in the past.

                assert!(&store.get(h + 1).is_ok());
                assert_eq!(store.get(h + 1).unwrap(), headers[h as usize + 1]);

                store.rollback(h).unwrap();

                assert!(
                    &store.get(h + 1).is_err(),
                    "after the rollback, we can't access blocks passed `h`"
                );
                assert_eq!(store.len().unwrap(), h as usize + 1);

                // We can now overwrite the block at position `h + 1`.
                let header = BlockHeader {
                    nonce: 49219374,
                    ..header
                };
                let height = store.put(iter::once(header)).unwrap();

                assert!(header != headers[height as usize]);

                assert_eq!(height, h + 1);
                assert_eq!(store.get(height).unwrap(), header);

                // Blocks up to and including `h` are unaffected by the rollback.
                assert_eq!(store.get(0).unwrap(), headers[0]);
                assert_eq!(store.get(1).unwrap(), headers[1]);
                assert_eq!(store.get(h).unwrap(), headers[h as usize]);
            }
        }

        #[test]
        fn test_iter() {
            let tmp = tempfile::tempdir().unwrap();
            let mut store = FileStore::open(tmp.path().join("headers.db")).unwrap();

            let count = 32;
            let header = BlockHeader {
                version: 1,
                prev_blockhash: Default::default(),
                merkle_root: Default::default(),
                bits: 0x2ffffff,
                time: 1842918273,
                nonce: 0,
            };
            let iter = (0..count).map(|i| BlockHeader { nonce: i, ..header });
            let headers = iter.clone().collect::<Vec<_>>();

            store.put(iter).unwrap();

            for result in store.iter() {
                let (height, header) = result.unwrap();

                assert_eq!(header, headers[height as usize]);
            }
        }
    }
}