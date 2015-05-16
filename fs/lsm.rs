﻿/*
    Copyright 2014-2015 Zumero, LLC

    Licensed under the Apache License, Version 2.0 (the "License");
    you may not use this file except in compliance with the License.
    You may obtain a copy of the License at

        http://www.apache.org/licenses/LICENSE-2.0

    Unless required by applicable law or agreed to in writing, software
    distributed under the License is distributed on an "AS IS" BASIS,
    WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
    See the License for the specific language governing permissions and
    limitations under the License.
*/

#![feature(collections)]
#![feature(box_syntax)]

use std::io;
use std::io::Seek;
use std::io::Read;
use std::io::Write;
use std::io::SeekFrom;

const size_i32 :usize = 4; // TODO
const size_i16 :usize = 2; // TODO

pub enum Blob {
    Stream(Box<Read>),
    Array(Box<[u8]>),
    Tombstone,
}

pub struct kvp {
    Key : Box<[u8]>,
    Value : Blob,
}

pub struct PendingSegment {
    blockList: Vec<PageBlock>
}

#[derive(Hash,PartialEq,Eq,Copy,Clone)]
pub struct PageBlock {
    firstPage : usize,
    lastPage : usize,
}

impl PageBlock {
    fn new(first : usize, last : usize) -> PageBlock {
        PageBlock { firstPage:first, lastPage:last }
    }

    fn CountPages(&self) -> usize {
        self.lastPage - self.firstPage + 1
    }
}

#[derive(Hash,PartialEq,Eq,Copy,Clone)]
pub struct Guid {
    a : [u8; 16]
}

impl Guid {
    fn new(ba: [u8; 16]) -> Guid {
        Guid { a: ba }
    }

    fn NewGuid() -> Guid {
        Guid { a:[0; 16] } // TODO
    }

    fn ToByteArray(&self) -> [u8;16] {
        self.a
    }
}

// TODO return Result
pub trait IPages {
    fn PageSize(&self) -> usize;
    fn Begin(&mut self) -> PendingSegment;
    fn GetBlock(&mut self, token:&mut PendingSegment) -> PageBlock;
    fn End(&mut self, token:PendingSegment, page:usize) -> Guid;
}

#[derive(PartialEq,Copy,Clone)]
enum SeekOp {
    SEEK_EQ = 0,
    SEEK_LE = 1,
    SEEK_GE = 2,
}

fn seek_len<R>(fs: &mut R) -> io::Result<u64> where R : Seek {
    let pos = try!(fs.seek(SeekFrom::Current(0)));
    let len = try!(fs.seek(SeekFrom::End(0)));
    let unused = try!(fs.seek(SeekFrom::Start(pos)));
    Ok(len)
}

// TODO return Result
trait ICursor : Drop {
    fn Seek(&mut self, k:&[u8], sop:SeekOp);
    fn First(&mut self);
    fn Last(&mut self);
    fn Next(&mut self);
    fn Prev(&mut self);

    fn IsValid(&self) -> bool;

    // TODO we wish Key() could return a reference, but the lifetime
    // would need to be "until the next call", and Rust can't really
    // do that.
    fn Key(&self) -> Box<[u8]>;

    // TODO similarly with Value().  When the Blob is an array, we would
    // prefer to return a reference to the bytes in the page.
    fn Value(&self) -> Blob;

    fn ValueLength(&self) -> i32; // because a negative length is a tombstone TODO option
    fn KeyCompare(&self, k:&[u8]) -> i32;

    fn CountKeysForward(&mut self) -> u32 {
        let mut i = 0;
        self.First();
        while self.IsValid() {
            i = i + 1;
            self.Next();
        }
        i
    }

    fn CountKeysBackward(&mut self) -> u32 {
        let mut i = 0;
        self.Last();
        while self.IsValid() {
            i = i + 1;
            self.Prev();
        }
        i
    }
}

impl Iterator for ICursor {
    type Item = kvp;
    fn next(& mut self) -> Option<kvp> {
        if self.IsValid() {
            return Some(kvp{Key:self.Key(), Value:self.Value()})
        } else {
            return None;
        }
    }
}

// TODO return Result
trait IWriteLock : Drop {
    fn CommitSegments(Iterator<Item=Guid>);
    fn CommitMerge(&Guid);
}

trait SeekRead : Seek + Read {
}

trait IDatabaseFile {
    fn OpenForReading() -> SeekRead;
    fn OpenForWriting() -> SeekRead;
}

struct DbSettings {
    AutoMergeEnabled : bool,
    AutoMergeMinimumPages : i32,
    DefaultPageSize : usize,
    PagesPerBlock : usize,
}

struct SegmentInfo {
    root : usize,
    age : u32,
    blocks : Vec<PageBlock>
}

trait IDatabase : Drop {
    fn WriteSegmentFromSortedSequence(q:Iterator<Item=kvp>) -> Guid;
    //fn WriteSegment : System.Collections.Generic.IDictionary<byte[],Stream> -> Guid;
    //fn WriteSegment : System.Collections.Generic.IDictionary<byte[],Blob> -> Guid;
    fn ForgetWaitingSegments(s:Iterator<Item=Guid>);

    fn GetFreeBlocks() -> Iterator<Item=PageBlock>;
    fn OpenCursor() -> ICursor; // why do we have to specify Item here?  and what lifetime?
    fn OpenSegmentCursor(Guid) ->ICursor;
    // TODO consider name such as OpenLivingCursorOnCurrentState()
    // TODO consider OpenCursorOnSegmentsInWaiting(seq<Guid>)
    // TODO consider ListSegmentsInCurrentState()
    // TODO consider OpenCursorOnSpecificSegment(seq<Guid>)

    // fn ListSegments : unit -> (Guid list)*Map<Guid,SegmentInfo>
    fn PageSize() -> usize;

    // fn RequestWriteLock : int->Async<IWriteLock>
    // fn RequestWriteLock : unit->Async<IWriteLock>

    // fn Merge : int*int*bool -> Async<Guid list> option
    // fn BackgroundMergeJobs : unit->Async<Guid list> list // TODO have Auto in the name of this?
}

mod utils {
    use std::io;
    use std::io::Seek;
    use std::io::Read;
    use std::io::Write;
    use std::io::SeekFrom;

    pub fn SeekPage(strm:&mut Seek, pageSize:usize, pageNumber:usize) -> io::Result<u64> {
        if 0==pageNumber { panic!("invalid page number") }
        let pos = (pageNumber - 1) * pageSize;
        strm.seek(SeekFrom::Start(pos as u64))
    }

    pub fn ReadFully(strm:&mut Read, buf: &mut [u8]) -> io::Result<usize> {
        let mut sofar = 0;
        let len = buf.len();
        loop {
            let cur = &mut buf[sofar..len];
            let n = try!(strm.read(cur));
            if n==0 {
                break;
            }
            sofar += n;
            if sofar==len {
                break;
            }
        }
        let res : io::Result<usize> = Ok(sofar);
        res
    }
}

mod bcmp {
    pub fn Compare (x:&[u8], y:&[u8]) -> i32 {
        let xlen = x.len();
        let ylen = y.len();
        let len = if xlen<ylen { xlen } else { ylen };
        let mut i = 0;
        while i<len {
            let c = (x[i] as i32) - (y[i] as i32);
            if c != 0 {
                return c;
            }
            else {
                i = i + 1;
            }
        }
        (xlen - ylen) as i32
    }

    pub fn CompareWithPrefix (prefix:&[u8], x:&[u8], y:&[u8]) -> i32 {
        let plen = prefix.len();
        let xlen = x.len();
        let ylen = y.len();
        let len = if xlen<ylen { xlen } else { ylen };
        let mut i = 0;
        while i<len {
            let xval = 
                if i<plen {
                    prefix[i]
                } else {
                    x[i - plen]
                };
            let c = (xval as i32) - (y[i] as i32);
            if c != 0 {
                return c;
            }
            else {
                i = i + 1;
            }
        }
        (xlen - ylen) as i32
    }

    pub fn PrefixMatch (x:&[u8], y:&[u8], max:usize) -> usize {
        let xlen = x.len();
        let ylen = y.len();
        let len = if xlen<ylen { xlen } else { ylen };
        let lim = if len<max { len } else { max };
        let mut i = 0;
        while i<lim && x[i]==y[i] {
            i = i + 1;
        }
        i
    }

    fn StartsWith (x:&[u8], y:&[u8], max:usize) -> bool {
        if x.len() < y.len() {
            false
        } else {
            let len = y.len();
            let mut i = 0;
            while i<len && x[i]==y[i] {
                i = i + 1;
            }
            i==len
        }
    }
}

mod Varint {
    pub fn SpaceNeededFor(v:u64) -> usize {
        if v<=240 { 1 }
        else if v<=2287 { 2 }
        else if v<=67823 { 3 }
        else if v<=16777215 { 4 }
        else if v<=4294967295 { 5 }
        else if v<=1099511627775 { 6 }
        else if v<=281474976710655 { 7 }
        else if v<=72057594037927935 { 8 }
        else { 9 }
    }

    pub fn read (buf:&[u8], cur:usize) -> (usize,u64) {
        let a0 = buf[cur] as u64;
        if a0 <= 240u64 { 
            (cur+1, a0)
        } else if a0 <= 248u64 {
            let a1 = buf[cur+1] as u64;
            let r = (240u64 + 256u64 * (a0 - 241u64) + a1);
            (cur+2, r)
        } else if a0 == 249u64 {
            let a1 = buf[cur+1] as u64;
            let a2 = buf[cur+2] as u64;
            let r = (2288u64 + 256u64 * a1 + a2);
            (cur+3, r)
        } else if a0 == 250u64 {
            let a1 = buf[cur+1] as u64;
            let a2 = buf[cur+2] as u64;
            let a3 = buf[cur+3] as u64;
            let r = (a1<<16) | (a2<<8) | a3;
            (cur+4, r)
        } else if a0 == 251u64 {
            let a1 = buf[cur+1] as u64;
            let a2 = buf[cur+2] as u64;
            let a3 = buf[cur+3] as u64;
            let a4 = buf[cur+4] as u64;
            let r = (a1<<24) | (a2<<16) | (a3<<8) | a4;
            (cur+5, r)
        } else if a0 == 252u64 {
            let a1 = buf[cur+1] as u64;
            let a2 = buf[cur+2] as u64;
            let a3 = buf[cur+3] as u64;
            let a4 = buf[cur+4] as u64;
            let a5 = buf[cur+5] as u64;
            let r = (a1<<32) | (a2<<24) | (a3<<16) | (a4<<8) | a5;
            (cur+6, r)
        } else if a0 == 253u64 {
            let a1 = buf[cur+1] as u64;
            let a2 = buf[cur+2] as u64;
            let a3 = buf[cur+3] as u64;
            let a4 = buf[cur+4] as u64;
            let a5 = buf[cur+5] as u64;
            let a6 = buf[cur+6] as u64;
            let r = (a1<<40) | (a2<<32) | (a3<<24) | (a4<<16) | (a5<<8) | a6;
            (cur+7, r)
        } else if a0 == 254u64 {
            let a1 = buf[cur+1] as u64;
            let a2 = buf[cur+2] as u64;
            let a3 = buf[cur+3] as u64;
            let a4 = buf[cur+4] as u64;
            let a5 = buf[cur+5] as u64;
            let a6 = buf[cur+6] as u64;
            let a7 = buf[cur+7] as u64;
            let r = (a1<<48) | (a2<<40) | (a3<<32) | (a4<<24) | (a5<<16) | (a6<<8) | a7;
            (cur+8, r)
        } else {
            let a1 = buf[cur+1] as u64;
            let a2 = buf[cur+2] as u64;
            let a3 = buf[cur+3] as u64;
            let a4 = buf[cur+4] as u64;
            let a5 = buf[cur+5] as u64;
            let a6 = buf[cur+6] as u64;
            let a7 = buf[cur+7] as u64;
            let a8 = buf[cur+8] as u64;
            let r = (a1<<56) | (a2<<48) | (a3<<40) | (a4<<32) | (a5<<24) | (a6<<16) | (a7<<8) | a8;
            (cur+9, r)
        }
    }

    pub fn write (buf:&mut [u8], cur:usize, v:u64) -> usize {
        if v<=240u64 { 
            buf[cur] = v as u8;
            cur + 1
        } else if v<=2287u64 { 
            buf[cur] = ((v - 240u64) / 256u64 + 241u64) as u8;
            buf[cur+1] = ((v - 240u64) % 256u64) as u8;
            cur + 2
        } else if v<=67823u64 { 
            buf[cur] = 249u8;
            buf[cur+1] = ((v - 2288u64) / 256u64) as u8;
            buf[cur+2] = ((v - 2288u64) % 256u64) as u8;
            cur + 3
        } else if v<=16777215u64 { 
            buf[cur] = 250u8;
            buf[cur+1] = (v >> 16) as u8;
            buf[cur+2] = (v >>  8) as u8;
            buf[cur+3] = (v >>  0) as u8;
            cur + 4
        } else if v<=4294967295u64 { 
            buf[cur] = 251u8;
            buf[cur+1] = (v >> 24) as u8;
            buf[cur+2] = (v >> 16) as u8;
            buf[cur+3] = (v >>  8) as u8;
            buf[cur+4] = (v >>  0) as u8;
            cur + 5
        } else if v<=1099511627775u64 { 
            buf[cur] = 252u8;
            buf[cur+1] = (v >> 32) as u8;
            buf[cur+2] = (v >> 24) as u8;
            buf[cur+3] = (v >> 16) as u8;
            buf[cur+4] = (v >>  8) as u8;
            buf[cur+5] = (v >>  0) as u8;
            cur + 6
        } else if v<=281474976710655u64 { 
            buf[cur] = 253u8;
            buf[cur+1] = (v >> 40) as u8;
            buf[cur+2] = (v >> 32) as u8;
            buf[cur+3] = (v >> 24) as u8;
            buf[cur+4] = (v >> 16) as u8;
            buf[cur+5] = (v >>  8) as u8;
            buf[cur+6] = (v >>  0) as u8;
            cur + 7
        } else if v<=72057594037927935u64 { 
            buf[cur] = 254u8;
            buf[cur+1] = (v >> 48) as u8;
            buf[cur+2] = (v >> 40) as u8;
            buf[cur+3] = (v >> 32) as u8;
            buf[cur+4] = (v >> 24) as u8;
            buf[cur+5] = (v >> 16) as u8;
            buf[cur+6] = (v >>  8) as u8;
            buf[cur+7] = (v >>  0) as u8;
            cur + 8
        } else {
            buf[cur] = 255u8;
            buf[cur+1] = (v >> 56) as u8;
            buf[cur+2] = (v >> 48) as u8;
            buf[cur+3] = (v >> 40) as u8;
            buf[cur+4] = (v >> 32) as u8;
            buf[cur+5] = (v >> 24) as u8;
            buf[cur+6] = (v >> 16) as u8;
            buf[cur+7] = (v >>  8) as u8;
            buf[cur+8] = (v >>  0) as u8;
            cur + 9
        }
    }
}

/*
fn push_i32_be(v:& mut Vec<u8>, i:i32)
{
    v.push((i>>24) as u8);
    v.push((i>>16) as u8);
    v.push((i>>8) as u8);
    v.push((i>>0) as u8);
}

fn push_i64_le(v:& mut Vec<u8>, i:i64)
{
    v.push((i>>0) as u8);
    v.push((i>>8) as u8);
    v.push((i>>16) as u8);
    v.push((i>>24) as u8);
    v.push((i>>32) as u8);
    v.push((i>>40) as u8);
    v.push((i>>48) as u8);
    v.push((i>>56) as u8);
}

fn push_i32_le(v:& mut Vec<u8>, i:i32)
{
    v.push((i>>0) as u8);
    v.push((i>>8) as u8);
    v.push((i>>16) as u8);
    v.push((i>>24) as u8);
}

fn push_cstring(v:& mut Vec<u8>, s:&String)
{
    v.push_all(s.as_bytes());
    v.push(0 as u8);
}
*/

fn write_i32_le(v:& mut [u8], i:i32)
{
    v[0] = (i>>0) as u8;
    v[1] = (i>>8) as u8;
    v[2] = (i>>16) as u8;
    v[3] = (i>>24) as u8;
}

fn write_i32_be(v:& mut [u8], i:i32)
{
    v[0] = (i>>24) as u8;
    v[1] = (i>>16) as u8;
    v[2] = (i>>8) as u8;
    v[3] = (i>>0) as u8;
}

fn read_i32_be(v:&[u8]) -> i32
{
    let a0 = v[0] as u64;
    let a1 = v[1] as u64;
    let a2 = v[2] as u64;
    let a3 = v[3] as u64;
    let r = (a0 << 24) | (a1 << 16) | (a2 << 8) | (a3 << 0);
    // assert r fits in a 32 bit signed int
    r as i32
}

fn read_i16_be(v:&[u8]) -> i16
{
    let a0 = v[0] as u64;
    let a1 = v[1] as u64;
    let r = (a0 << 8) | (a1 << 0);
    // assert r fits in a 16 bit signed int
    r as i16
}

fn write_i16_be(v:& mut [u8], i:i16)
{
    v[0] = (i>>8) as u8;
    v[1] = (i>>0) as u8;
}

struct PageBuilder {
    cur : usize,
    buf : Box<[u8]>,
}

// TODO bundling cur with the buf almost seems sad, because there are
// cases where we want buf to be mutable but not cur.  :-)

impl PageBuilder {
    fn new(pgsz : usize) -> PageBuilder { 
        let mut ba = vec![0;pgsz].into_boxed_slice();
        PageBuilder { cur:0, buf:ba } 
    }

    fn Reset(&mut self) {
        self.cur = 0;
    }

    fn Write(&self, strm:&mut Write) -> io::Result<()> {
        strm.write_all(&*self.buf)
    }

    fn PageSize(&self) -> usize {
        self.buf.len()
    }

    fn Buffer(&self) -> &[u8] {
        &self.buf
    }
    
    fn Position(&self) -> usize {
        self.cur
    }

    fn Available(&self) -> usize {
        self.buf.len() - self.cur
    }

    fn SetPageFlag(&mut self, x:u8) {
        self.buf[1] = self.buf[1] | (x);
    }

    fn PutByte(&mut self, x:u8) {
        self.buf[self.cur] = x;
        self.cur = self.cur + 1;
    }

    fn PutStream2(&mut self, s:&mut Read, len:usize) -> io::Result<usize> {
        let n = try!(utils::ReadFully(s, &mut self.buf[self.cur .. self.cur + len]));
        self.cur = self.cur + n;
        let res : io::Result<usize> = Ok(n);
        res
    }

    fn PutStream(&mut self, s:&mut Read, len:usize) -> io::Result<usize> {
        let n = try!(self.PutStream2(s, len));
        // TODO if n != len fail
        let res : io::Result<usize> = Ok(len);
        res
    }

    fn PutArray(&mut self, ba:&[u8]) {
        // TODO this can't be the best way to copy a slice
        for i in 0..ba.len() {
            self.buf[self.cur + i] = ba[i];
        }
        self.cur = self.cur + ba.len();
    }

    // TODO should be u32
    fn PutInt32(&mut self, ov:i32) {
        let at = self.cur;
        write_i32_be(&mut self.buf[at .. at+size_i32], ov);
        self.cur = self.cur + size_i32;
    }

    // TODO should be u32
    fn SetSecondToLastInt32(&mut self, page:i32) {
        let len = self.buf.len();
        let at = len - 2 * size_i32;
        if self.cur > at { panic!("SetSecondToLastInt32 is squashing data"); }
        write_i32_be(&mut self.buf[at .. at+size_i32], page);
    }

    // TODO should be u32
    fn SetLastInt32(&mut self, page:i32) {
        let len = self.buf.len();
        let at = len - 1 * size_i32;
        if self.cur > at { panic!("SetLastInt32 is squashing data"); }
        write_i32_be(&mut self.buf[at .. at+size_i32], page);
    }

    fn PutInt16(&mut self, ov:i16) {
        let at = self.cur;
        write_i16_be(&mut self.buf[at .. at+size_i16], ov);
        self.cur = self.cur + size_i16;
    }

    fn PutInt16At(&mut self, at:usize, ov:i16) {
        write_i16_be(&mut self.buf[at .. at+size_i16], ov);
    }

    fn PutVarint(&mut self, ov:u64) {
        self.cur = Varint::write(&mut *self.buf, self.cur, ov);
    }

}

struct PageReader {
    cur : usize,
    buf : Box<[u8]>,
}

impl PageReader {
    fn new(pgsz : usize) -> PageReader { 
        let mut ba = vec![0;pgsz].into_boxed_slice();
        PageReader { cur:0, buf:ba } 
    }

    pub fn Position(&self) -> usize {
        self.cur
    }

    fn PageSize(&self) -> usize {
        self.buf.len()
    }

    fn SetPosition(&mut self, x:usize) {
        self.cur = x;
    }

    fn Read(&mut self, strm:&mut Read) -> io::Result<usize> {
        utils::ReadFully(strm, &mut self.buf)
    }

    fn ReadPart(&mut self, strm:&mut Read, off: usize, len: usize) -> io::Result<usize> {
        utils::ReadFully(strm, &mut self.buf[off .. len-off])
    }

    fn Reset(&mut self) {
        self.cur = 0;
    }

    fn Compare(&self, len: usize, other: &[u8]) ->i32 {
        let slice = &self.buf[self.cur .. self.cur + len];
        bcmp::Compare(slice, other)
    }

    fn CompareWithPrefix(&self, prefix: &[u8], len: usize, other: &[u8]) ->i32 {
        let slice = &self.buf[self.cur .. self.cur + len];
        bcmp::CompareWithPrefix(prefix, slice, other)
    }

    fn PageType(&self) -> u8 {
        self.buf[0]
    }

    fn Skip(&mut self, len:usize) {
        self.cur = self.cur + len;
    }

    fn GetByte(&mut self) -> u8 {
        let r = self.buf[self.cur];
        self.cur = self.cur + 1;
        r
    }

    fn GetInt32(&mut self) -> i32 {
        let at = self.cur;
        let r = read_i32_be(&self.buf[at .. at+size_i32]);
        self.cur = self.cur + size_i32;
        r
    }

    fn GetInt32At(&self, at:usize) -> i32 {
        read_i32_be(&self.buf[at .. at+size_i32])
    }

    fn CheckPageFlag(&self, f:u8) -> bool {
        0 != (self.buf[1] & f)
    }

    fn GetSecondToLastInt32(&self) -> i32 {
        let len = self.buf.len();
        let at = len - 2 * size_i32;
        self.GetInt32At(at)
    }

    fn GetLastInt32(&self) -> i32 {
        let len = self.buf.len();
        let at = len - 1 * size_i32;
        self.GetInt32At(at)
    }

    fn GetInt16(&mut self) -> i16 {
        let at = self.cur;
        let r = read_i16_be(&self.buf[at .. at+size_i16]);
        self.cur = self.cur + size_i16;
        r
    }

    fn GetIntoArray(&self, a : &mut [u8]) {
        // TODO copy slice
        for i in 0 .. a.len() {
            a[i] = self.buf[self.cur + i];
        }
        // TODO advance cur
    }

    fn GetVarint(&mut self) -> u64 {
        let (newCur, v) = Varint::read(&*self.buf, self.cur);
        self.cur = newCur;
        v
    }

}

struct PageBuffer {
    buf : Box<[u8]>,
}

impl PageBuffer {
    fn new(pgsz : usize) -> PageBuffer { 
        let mut ba = vec![0;pgsz].into_boxed_slice();
        PageBuffer { buf:ba } 
    }

    fn PageSize(&self) -> usize {
        self.buf.len()
    }

    fn Read(&mut self, strm:&mut Read) -> io::Result<usize> {
        utils::ReadFully(strm, &mut self.buf)
    }

    fn ReadPart(&mut self, strm:&mut Read, off: usize, len: usize) -> io::Result<usize> {
        utils::ReadFully(strm, &mut self.buf[off .. len-off])
    }

    fn Compare(&self, cur: usize, len: usize, other: &[u8]) ->i32 {
        let slice = &self.buf[cur .. cur + len];
        bcmp::Compare(slice, other)
    }

    fn CompareWithPrefix(&self, cur: usize, prefix: &[u8], len: usize, other: &[u8]) ->i32 {
        let slice = &self.buf[cur .. cur + len];
        bcmp::CompareWithPrefix(prefix, slice, other)
    }

    fn PageType(&self) -> u8 {
        self.buf[0]
    }

    fn GetByte(&self, cur: &mut usize) -> u8 {
        let r = self.buf[*cur];
        *cur = *cur + 1;
        r
    }

    fn GetInt32(&self, cur: &mut usize) -> i32 {
        let at = *cur;
        let r = read_i32_be(&self.buf[at .. at+size_i32]);
        *cur = *cur + size_i32;
        r
    }

    fn GetInt32At(&self, at: usize) -> i32 {
        read_i32_be(&self.buf[at .. at+size_i32])
    }

    fn CheckPageFlag(&self, f: u8) -> bool {
        0 != (self.buf[1] & f)
    }

    fn GetSecondToLastInt32(&self) -> i32 {
        let len = self.buf.len();
        let at = len - 2 * size_i32;
        self.GetInt32At(at)
    }

    fn GetLastInt32(&self) -> i32 {
        let len = self.buf.len();
        let at = len - 1 * size_i32;
        self.GetInt32At(at)
    }

    fn GetInt16(&self, cur: &mut usize) -> i16 {
        let at = *cur;
        let r = read_i16_be(&self.buf[at .. at+size_i16]);
        *cur = *cur + size_i16;
        r
    }

    fn GetIntoArray(&self, cur: &mut usize,  a : &mut [u8]) {
        // TODO copy slice
        for i in 0 .. a.len() {
            a[i] = self.buf[*cur + i];
        }
        *cur = *cur + a.len();
    }

    fn GetVarint(&self, cur: &mut usize) -> u64 {
        let (newCur, v) = Varint::read(&*self.buf, *cur);
        *cur = newCur;
        v
    }

}

#[derive(PartialEq,Copy,Clone)]
enum Direction {
    FORWARD = 0,
    BACKWARD = 1,
    WANDERING = 2,
}

struct MultiCursor { 
    subcursors : Box<[Box<ICursor>]>, 
    cur : Option<usize>,
    dir : Direction,
}

impl MultiCursor {
    fn find(&self, compare_func : &Fn(&ICursor,&ICursor) -> i32) -> Option<usize> {
        if self.subcursors.is_empty() {
            None
        } else {
            let mut res = None::<usize>;
            for i in 0 .. self.subcursors.len() {
                match res {
                    Some(winning) => {
                        let x = &self.subcursors[i];
                        let y = &self.subcursors[winning];
                        let c = compare_func(&**x,&**y);
                        if c<0 {
                            res = Some(i)
                        }
                    },
                    None => {
                        res = Some(i)
                    }
                }
            }
            res
        }
    }

    fn findMin(&self) -> Option<usize> {
        let compare_func = |a:&ICursor,b:&ICursor| a.KeyCompare(&*b.Key());
        self.find(&compare_func)
    }

    fn findMax(&self) -> Option<usize> {
        let compare_func = |a:&ICursor,b:&ICursor| b.KeyCompare(&*a.Key());
        self.find(&compare_func)
    }

    fn Create(subs: Vec<Box<ICursor>>) -> MultiCursor {
        let s = subs.into_boxed_slice();
        MultiCursor { subcursors: s, cur : None, dir : Direction::WANDERING }
    }

}

impl Drop for MultiCursor {
    fn drop(&mut self) {
        // TODO
        println!("Dropping!");
    }
}

impl ICursor for MultiCursor {
    fn IsValid(&self) -> bool {
        match self.cur {
            Some(i) => self.subcursors[i].IsValid(),
            None => false
        }
    }

    fn First(&mut self) {
        for i in 0 .. self.subcursors.len() {
            self.subcursors[i].First();
        }
        self.cur = self.findMin();
        self.dir = Direction::WANDERING; // TODO why?
    }

    fn Last(&mut self) {
        for i in 0 .. self.subcursors.len() {
            self.subcursors[i].Last();
        }
        self.cur = self.findMax();
        self.dir = Direction::WANDERING; // TODO why?
    }

    // the following members are designed to panic if called when
    // the cursor is not valid.
    // this matches the C# behavior and the expected behavior of ICursor.
    // don't call these methods without checking IsValid() first.

    fn Key(&self) -> Box<[u8]> {
        match self.cur {
            Some(icur) => self.subcursors[icur].Key(),
            None => panic!()
        }
    }

    fn KeyCompare(&self, k:&[u8]) -> i32 {
        match self.cur {
            Some(icur) => self.subcursors[icur].KeyCompare(k),
            None => panic!()
        }
    }

    fn Value(&self) -> Blob {
        match self.cur {
            Some(icur) => self.subcursors[icur].Value(),
            None => panic!()
        }
    }

    fn ValueLength(&self) -> i32 {
        match self.cur {
            Some(icur) => self.subcursors[icur].ValueLength(),
            None => panic!()
        }
    }

    fn Next(&mut self) {
        match self.cur {
            Some(icur) => {
                let k = self.subcursors[icur].Key();
                for j in 0 .. self.subcursors.len() {
                    let csr = &mut self.subcursors[j];
                    if (self.dir != Direction::FORWARD) && (icur != j) { 
                        (*csr).Seek (&*k, SeekOp::SEEK_GE); 
                    }
                    if csr.IsValid() && (0 == csr.KeyCompare(&*k)) { 
                        csr.Next(); 
                    }
                }
                self.cur = self.findMin();
                self.dir = Direction::FORWARD;
            },
            None => panic!()
        }
    }

    fn Prev(&mut self) {
        match self.cur {
            Some(icur) => {
                let k = self.subcursors[icur].Key();
                for j in 0 .. self.subcursors.len() {
                    let csr = &mut self.subcursors[j];
                    if (self.dir != Direction::BACKWARD) && (icur != j) { 
                        (*csr).Seek (&*k, SeekOp::SEEK_LE); 
                    }
                    if csr.IsValid() && (0 == csr.KeyCompare(&*k)) { 
                        csr.Prev(); 
                    }
                }
            },
            None => panic!()
        }
        self.cur = self.findMax();
        self.dir = Direction::BACKWARD;
    }

    fn Seek(&mut self, k:&[u8], sop:SeekOp) {
        self.cur = None;
        self.dir = Direction::WANDERING;
        let mut found = false;
        for j in 0 .. self.subcursors.len() {
            self.subcursors[j].Seek(k,sop);
            if self.cur.is_none() && self.subcursors[j].IsValid() && ( (SeekOp::SEEK_EQ == sop) || (0 == self.subcursors[j].KeyCompare (k)) ) { 
                self.cur = Some(j);
                found = true;
                break;
            }
        }
        if !found {
            match sop {
                SeekOp::SEEK_GE => {
                    self.cur = self.findMin();
                    if self.cur.is_some() { 
                        self.dir = Direction::FORWARD; 
                    }
                },
                SeekOp::SEEK_LE => {
                    self.cur = self.findMax();
                    if self.cur.is_some() { 
                        self.dir = Direction::BACKWARD; 
                    }
                },
                SeekOp::SEEK_EQ => ()
            }
        }
    }

}

struct LivingCursor { 
    chain : Box<ICursor>
}

impl LivingCursor {
    fn skipTombstonesForward(&mut self) {
        while self.chain.IsValid() && self.chain.ValueLength()<0 {
            self.chain.Next();
        }
    }

    fn skipTombstonesBackward(&mut self) {
        while self.chain.IsValid() && self.chain.ValueLength()<0 {
            self.chain.Prev();
        }
    }

    pub fn Create(ch : Box<ICursor>) -> LivingCursor {
        LivingCursor { chain : ch }
    }
}

impl Drop for LivingCursor {
    fn drop(&mut self) {
        // TODO
        println!("Dropping!");
    }
}

impl ICursor for LivingCursor {
    fn First(&mut self) {
        self.chain.First();
        self.skipTombstonesForward();
    }

    fn Last(&mut self) {
        self.chain.Last();
        self.skipTombstonesBackward();
    }

    fn Key(&self) -> Box<[u8]> {
        self.chain.Key()
    }

    fn Value(&self) -> Blob {
        self.chain.Value()
    }

    fn ValueLength(&self) -> i32 {
        self.chain.ValueLength()
    }

    fn IsValid(&self) -> bool {
        self.chain.IsValid() && self.chain.ValueLength() >= 0
    }

    fn KeyCompare(&self, k:&[u8]) -> i32 {
        self.chain.KeyCompare(k)
    }

    fn Next(&mut self) {
        self.chain.Next();
        self.skipTombstonesForward();
    }

    fn Prev(&mut self) {
        self.chain.Prev();
        self.skipTombstonesBackward();
    }

    fn Seek(&mut self, k:&[u8], sop:SeekOp) {
        self.chain.Seek(k, sop);
        match sop {
            SeekOp::SEEK_GE => self.skipTombstonesForward(),
            SeekOp::SEEK_LE => self.skipTombstonesBackward(),
            SeekOp::SEEK_EQ => (),
        }
    }

}

mod bt {

    use std::io::Write;
    use std::collections::HashMap;

    use super::PageBlock;

    // page types
    mod PageType {
        pub const LEAF_NODE: u8 = 1;
        pub const PARENT_NODE: u8 = 2;
        pub const OVERFLOW_NODE: u8 = 3;
    }

    // flags on values
    mod ValueFlag {
        pub const FLAG_OVERFLOW: u8 = 1;
        pub const FLAG_TOMBSTONE: u8 = 2;
    }

    // flags on pages
    mod PageFlag {
        pub const FLAG_ROOT_NODE: u8 = 1;
        pub const FLAG_BOUNDARY_NODE: u8 = 2;
        pub const FLAG_ENDS_ON_BOUNDARY: u8 = 3;
    }

    struct pgitem {
        page : usize,
        key : Box<[u8]>, // TODO reference instead of box?
        // TODO constructor impl ?
    }

    struct ParentState {
        sofar : usize,
        nextGeneration : Vec<pgitem>,
        blk : PageBlock,
    }

    // TODO gratuitously different names of the items in these
    // two unions

    enum KeyLocation {
        Inline,
        Overflow(usize),
    }

    enum ValueLocation {
        Tombstone,
        Buffer(Box<[u8]>), // TODO reference instead of box?
        Overflowed(usize,usize),
    }

    struct LeafPair {
        key : Box<[u8]>,
        kLoc : KeyLocation,
        vLoc : ValueLocation,
    }

    struct LeafState {
        sofarLeaf : usize,
        keys : Vec<Box<LeafPair>>,
        prevLeaf : usize,
        prefixLen : usize,
        firstLeaf : usize,
        leaves : Vec<pgitem>,
        blk : PageBlock,
    }

    use super::utils;
    use super::IPages;
    use super::kvp;
    use super::PageBuilder;
    use std::io;
    use std::io::Read;
    use std::io::Seek;
    use super::PendingSegment;
    use super::Varint;
    use super::Blob;
    use super::bcmp;
    use super::Guid;
    use super::size_i32;

    pub fn CreateFromSortedSequenceOfKeyValuePairs<I,SeekWrite>(fs: &mut SeekWrite, 
                                                                pageManager: &mut IPages, 
                                                                source: I,
                                                               ) -> io::Result<(Guid,usize)> where I:Iterator<Item=kvp>, SeekWrite : Seek+Write {

        fn writeOverflow<SeekWrite>(startingBlock: PageBlock, 
                                    ba: &mut Read, 
                                    pageManager: &mut IPages, 
                                    fs: &mut SeekWrite
                                   ) -> io::Result<(usize,PageBlock)> where SeekWrite : Seek+Write {
            fn buildFirstPage(ba:&mut Read, pbFirstOverflow : &mut PageBuilder, pageSize : usize) -> io::Result<(usize,bool)> {
                pbFirstOverflow.Reset();
                pbFirstOverflow.PutByte(PageType::OVERFLOW_NODE as u8);
                pbFirstOverflow.PutByte(0u8); // starts 0, may be changed later
                let room = (pageSize - (2 + size_i32));
                // something will be put in lastInt32 later
                match pbFirstOverflow.PutStream2(ba, room) {
                    Ok(put) => Ok((put, put<room)),
                    Err(e) => Err(e),
                }
            };

            fn buildRegularPage(ba:&mut Read, pbOverflow : &mut PageBuilder, pageSize : usize) -> io::Result<(usize,bool)> {
                pbOverflow.Reset();
                let room = pageSize;
                match pbOverflow.PutStream2(ba, room) {
                    Ok(put) => Ok((put, put<room)),
                    Err(e) => Err(e),
                }
            };

            fn buildBoundaryPage(ba:&mut Read, pbOverflow : &mut PageBuilder, pageSize : usize) -> io::Result<(usize,bool)> {
                pbOverflow.Reset();
                let room = (pageSize - size_i32);
                // something will be put in lastInt32 before the page is written
                match pbOverflow.PutStream2(ba, room) {
                    Ok(put) => Ok((put, put<room)),
                    Err(e) => Err(e),
                }
            }

            fn writeRegularPages<SeekWrite>(max :usize, 
                                            sofar :usize, 
                                            pb : &mut PageBuilder, 
                                            fs : &mut SeekWrite, 
                                            ba : &mut Read, 
                                            pageSize : usize
                                           ) -> io::Result<(usize,usize,bool)> where SeekWrite : Seek+Write {
                let mut i = 0;
                loop {
                    if i < max {
                        let (put, finished) = try!(buildRegularPage(ba, pb, pageSize));
                        if put==0 {
                            return Ok((i, sofar, true));
                        } else {
                            let sofar = sofar + put;
                            pb.Write(fs);
                            if finished {
                                return Ok((i+1, sofar, true));
                            } else {
                                i = i + 1;
                            }
                        }
                    } else {
                        return Ok((i, sofar, false));
                    }
                }
            }

            // TODO misnamed
            fn writeOneBlock<SeekWrite>(param_sofar: usize, 
                             param_firstBlk: PageBlock,
                             fs: &mut SeekWrite, 
                             ba: &mut Read, 
                             pageSize: usize,
                             pbOverflow: &mut PageBuilder,
                             pbFirstOverflow: &mut PageBuilder,
                             pageManager: &mut IPages,
                             token: &mut PendingSegment
                             ) -> io::Result<(usize,PageBlock)> where SeekWrite : Seek+Write {
                // each trip through this loop will write out one
                // block, starting with the overflow first page,
                // followed by zero-or-more "regular" overflow pages,
                // which have no header.  we'll stop at the block boundary,
                // either because we land there or because the whole overflow
                // won't fit and we have to continue into the next block.
                // the boundary page will be like a regular overflow page,
                // headerless, but it is four bytes smaller.
                let mut loop_sofar = param_sofar;
                let mut loop_firstBlk = param_firstBlk;
                loop {
                    let sofar = loop_sofar;
                    let firstBlk = loop_firstBlk;
                    let (putFirst,finished) = try!(buildFirstPage (ba, pbFirstOverflow, pageSize));
                    if putFirst==0 { 
                        return Ok((sofar, firstBlk));
                    } else {
                        // note that we haven't written the first page yet.  we may have to fix
                        // a couple of things before it gets written out.
                        let sofar = sofar + putFirst;
                        if firstBlk.firstPage == firstBlk.lastPage {
                            // the first page landed on a boundary.
                            // we can just set the flag and write it now.
                            pbFirstOverflow.SetPageFlag(PageFlag::FLAG_BOUNDARY_NODE as u8);
                            let blk = pageManager.GetBlock(&mut *token);
                            pbFirstOverflow.SetLastInt32(blk.firstPage as i32);
                            pbFirstOverflow.Write(fs);
                            utils::SeekPage(fs, pageSize, blk.firstPage);
                            if !finished {
                                loop_sofar = sofar;
                                loop_firstBlk = blk;
                            } else {
                                return Ok((sofar, blk));
                            }
                        } else {
                            let firstRegularPageNumber = (firstBlk.firstPage + 1) as usize;
                            if finished {
                                // the first page is also the last one
                                pbFirstOverflow.SetLastInt32(0); // offset to last used page in this block, which is this one
                                pbFirstOverflow.Write(fs);
                                return Ok((sofar, PageBlock::new(firstRegularPageNumber,firstBlk.lastPage)));
                            } else {
                                // we need to write more pages,
                                // until the end of the block,
                                // or the end of the stream, 
                                // whichever comes first

                                utils::SeekPage(fs, pageSize, firstRegularPageNumber);

                                // availableBeforeBoundary is the number of pages until the boundary,
                                // NOT counting the boundary page, and the first page in the block
                                // has already been accounted for, so we're just talking about data pages.
                                let availableBeforeBoundary = 
                                    if firstBlk.lastPage > 0 
                                        { (firstBlk.lastPage - firstRegularPageNumber) }
                                    else 
                                        { usize::max_value() }
                                    ;

                                let (numRegularPages, sofar, finished) = 
                                    try!(writeRegularPages(availableBeforeBoundary, sofar, pbOverflow, fs, ba, pageSize));

                                if finished {
                                    // go back and fix the first page
                                    pbFirstOverflow.SetLastInt32(numRegularPages as i32);
                                    utils::SeekPage(fs, pageSize, firstBlk.firstPage);
                                    pbFirstOverflow.Write(fs);
                                    // now reset to the next page in the block
                                    let blk = PageBlock::new(firstRegularPageNumber + numRegularPages, firstBlk.lastPage);
                                    utils::SeekPage(fs, pageSize, blk.firstPage);
                                    return Ok((sofar,blk));
                                } else {
                                    // we need to write out a regular page except with a
                                    // boundary pointer in it.  and we need to set
                                    // FLAG_ENDS_ON_BOUNDARY on the first
                                    // overflow page in this block.

                                    let (putBoundary,finished) = try!(buildBoundaryPage (ba, pbOverflow, pageSize));
                                    if putBoundary==0 {
                                        // go back and fix the first page
                                        pbFirstOverflow.SetLastInt32(numRegularPages as i32);
                                        utils::SeekPage(fs, pageSize, firstBlk.firstPage);
                                        pbFirstOverflow.Write(fs);

                                        // now reset to the next page in the block
                                        let blk = PageBlock::new(firstRegularPageNumber + numRegularPages, firstBlk.lastPage);
                                        utils::SeekPage(fs, pageSize, firstBlk.lastPage);
                                        return Ok((sofar,blk));
                                    } else {
                                        // write the boundary page
                                        let sofar = sofar + putBoundary;
                                        let blk = pageManager.GetBlock(&mut *token);
                                        pbOverflow.SetLastInt32(blk.firstPage as i32);
                                        pbOverflow.Write(fs);

                                        // go back and fix the first page
                                        pbFirstOverflow.SetPageFlag(PageFlag::FLAG_ENDS_ON_BOUNDARY as u8);
                                        pbFirstOverflow.SetLastInt32((numRegularPages + 1) as i32);
                                        utils::SeekPage(fs, pageSize, firstBlk.firstPage);
                                        pbFirstOverflow.Write(fs);

                                        // now reset to the first page in the next block
                                        utils::SeekPage(fs, pageSize, blk.firstPage);
                                        if finished {
                                            loop_sofar = sofar;
                                            loop_firstBlk = blk;
                                        } else {
                                            return Ok((sofar,blk));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let pageSize = pageManager.PageSize();
            let mut token = pageManager.Begin();
            let mut pbFirstOverflow = PageBuilder::new(pageSize);
            let mut pbOverflow = PageBuilder::new(pageSize);

            writeOneBlock(0, startingBlock, fs, ba, pageSize, &mut pbOverflow, &mut pbFirstOverflow, pageManager, &mut token)
        }

        fn writeLeaves<I,SeekWrite>(leavesBlk:PageBlock,
                                    pageManager: &mut IPages,
                                    source: I,
                                    vbuf: &mut [u8],
                                    fs: &mut SeekWrite, 
                                    pb: &mut PageBuilder,
                                    token: &mut PendingSegment,
                                    ) -> io::Result<(PageBlock,Vec<pgitem>,usize)> where I: Iterator<Item=kvp> , SeekWrite : Seek+Write {
            // 2 for the page type and flags
            // 4 for the prev page
            // 2 for the stored count
            // 4 for lastInt32 (which isn't in pb.Available)
            let LEAF_PAGE_OVERHEAD = 2 + 4 + 2 + 4;

            fn buildLeaf(st: &LeafState, pb: &mut PageBuilder) {
                pb.Reset();
                pb.PutByte(PageType::LEAF_NODE as u8);
                pb.PutByte(0u8); // flags
                pb.PutInt32 (st.prevLeaf as i32); // prev page num.
                // TODO prefixLen is one byte.  should it be two?
                pb.PutByte(st.prefixLen as u8);
                if st.prefixLen > 0 {
                    pb.PutArray(&st.keys[0].key[0 .. st.prefixLen]);
                }
                pb.PutInt16 (st.keys.len() as i16);
                for lp in &st.keys {
                    match lp.kLoc {
                        KeyLocation::Inline => {
                            pb.PutByte(0u8); // flags
                            pb.PutVarint(lp.key.len() as u64);
                            pb.PutArray(&lp.key[st.prefixLen .. lp.key.len()]);
                        },
                        KeyLocation::Overflow(kpage) => {
                            pb.PutByte(ValueFlag::FLAG_OVERFLOW as u8);
                            pb.PutVarint(lp.key.len() as u64);
                            pb.PutInt32(kpage as i32);
                        },
                    }
                    match lp.vLoc {
                        ValueLocation::Tombstone => {
                            pb.PutByte(ValueFlag::FLAG_TOMBSTONE as u8);
                        },
                        ValueLocation::Buffer (ref vbuf) => {
                            pb.PutByte(0u8);
                            pb.PutVarint(vbuf.len() as u64);
                            pb.PutArray(&vbuf);
                        },
                        ValueLocation::Overflowed (vlen,vpage) => {
                            pb.PutByte(ValueFlag::FLAG_OVERFLOW as u8);
                            pb.PutVarint(vlen as u64);
                            pb.PutInt32(vpage as i32);
                        },
                    }
                }
            }

            fn writeLeaf<SeekWrite>(st: &mut LeafState, 
                         isRootPage: bool, 
                         pb: &mut PageBuilder, 
                         fs: &mut SeekWrite, 
                         pageSize: usize,
                         pageManager: &mut IPages,
                         token: &mut PendingSegment,
                         ) where SeekWrite : Seek+Write { 
                buildLeaf(st, pb);
                let thisPageNumber = st.blk.firstPage;
                let firstLeaf = if st.leaves.is_empty() { thisPageNumber } else { st.firstLeaf };
                let nextBlk = 
                    if isRootPage {
                        PageBlock::new(thisPageNumber + 1, st.blk.lastPage)
                    } else if thisPageNumber == st.blk.lastPage {
                        pb.SetPageFlag(PageFlag::FLAG_BOUNDARY_NODE as u8);
                        let newBlk = pageManager.GetBlock(&mut *token);
                        pb.SetLastInt32(newBlk.firstPage as i32);
                        newBlk
                    } else {
                        PageBlock::new(thisPageNumber + 1, st.blk.lastPage)
                    };
                pb.Write(fs);
                if nextBlk.firstPage != (thisPageNumber+1) {
                    utils::SeekPage(fs, pageSize, nextBlk.firstPage);
                }
                // TODO isn't there a better way to copy a slice?
                let mut ba = Vec::new();
                ba.push_all(&st.keys[0].key);
                let pg = pgitem {page:thisPageNumber, key:ba.into_boxed_slice()};
                st.leaves.push(pg);
                st.sofarLeaf = 0;
                st.keys = Vec::new();
                st.prevLeaf = thisPageNumber;
                st.prefixLen = 0;
                st.firstLeaf = firstLeaf;
                st.blk = nextBlk;
            }

            // TODO can the overflow page number become a varint?
            const neededForOverflowPageNumber: usize = 4;

            // the max limit of an inline key is when that key is the only
            // one in the leaf, and its value is overflowed.

            let pageSize = pageManager.PageSize();
            let maxKeyInline = 
                pageSize 
                - LEAF_PAGE_OVERHEAD 
                - 1 // prefixLen
                - 1 // key flags
                - Varint::SpaceNeededFor(pageSize as u64) // approx worst case inline key len
                - 1 // value flags
                - 9 // worst case varint value len
                - neededForOverflowPageNumber; // overflowed value page

            fn kLocNeed(k: &[u8], kloc: &KeyLocation, prefixLen: usize) -> usize {
                let klen = k.len();
                match *kloc {
                    KeyLocation::Inline => {
                        1 + Varint::SpaceNeededFor(klen as u64) + klen - prefixLen
                    },
                    KeyLocation::Overflow(_) => {
                        1 + Varint::SpaceNeededFor(klen as u64) + neededForOverflowPageNumber
                    },
                }
            }

            fn vLocNeed (vloc: &ValueLocation) -> usize {
                match *vloc {
                    ValueLocation::Tombstone => {
                        1
                    },
                    ValueLocation::Buffer(ref vbuf) => {
                        let vlen = vbuf.len();
                        1 + Varint::SpaceNeededFor(vlen as u64) + vlen
                    },
                    ValueLocation::Overflowed(vlen,_) => {
                        1 + Varint::SpaceNeededFor(vlen as u64) + neededForOverflowPageNumber
                    },
                }
            }

            fn leafPairSize(prefixLen: usize, lp: &LeafPair) -> usize {
                kLocNeed(&lp.key, &lp.kLoc, prefixLen)
                +
                vLocNeed(&lp.vLoc)
            }

            fn defaultPrefixLen (k:&[u8]) -> usize {
                // TODO max prefix.  relative to page size?  must fit in byte.
                if k.len() > 255 { 255 } else { k.len() }
            }

            // this is the body of writeLeaves
            //let source = seq { csr.First(); while csr.IsValid() do yield (csr.Key(), csr.Value()); csr.Next(); done }
            let mut st = LeafState {
                sofarLeaf:0,
                firstLeaf:0,
                prevLeaf:0,
                keys:Vec::new(),
                prefixLen:0,
                leaves:Vec::new(),
                blk:leavesBlk,
                };

            for mut pair in source {
                let k = pair.Key;
                // assert k <> null
                // but pair.Value might be null (a tombstone)

                // TODO is it possible for this to conclude that the key must be overflowed
                // when it would actually fit because of prefixing?

                let (blkAfterKey,kloc) = 
                    if k.len() <= maxKeyInline {
                        (st.blk, KeyLocation::Inline)
                    } else {
                        let vPage = st.blk.firstPage;
                        let (_,newBlk) = try!(writeOverflow(st.blk, &mut &*k, pageManager, fs));
                        (newBlk, KeyLocation::Overflow(vPage))
                    };

                // the max limit of an inline value is when the key is inline
                // on a new page.

                let availableOnNewPageAfterKey = 
                    pageSize 
                    - LEAF_PAGE_OVERHEAD 
                    - 1 // prefixLen
                    - 1 // key flags
                    - Varint::SpaceNeededFor(k.len() as u64)
                    - k.len() 
                    - 1 // value flags
                    ;

                // availableOnNewPageAfterKey needs to accomodate the value and its length as a varint.
                // it might already be <=0 because of the key length

                let maxValueInline = 
                    if availableOnNewPageAfterKey > 0 {
                        let neededForVarintLen = Varint::SpaceNeededFor(availableOnNewPageAfterKey as u64);
                        let avail2 = availableOnNewPageAfterKey - neededForVarintLen;
                        if avail2 > 0 { avail2 } else { 0 }
                    } else {
                        0
                    };

                let (blkAfterValue, vloc) = 
                    match pair.Value {
                        Blob::Tombstone => {
                            (blkAfterKey, ValueLocation::Tombstone)
                        },
                        _ => match kloc {
                             KeyLocation::Inline => {
                                if maxValueInline == 0 {
                                    match pair.Value {
                                        Blob::Tombstone => {
                                            (blkAfterKey, ValueLocation::Tombstone)
                                        },
                                        Blob::Stream(ref mut strm) => {
                                            let valuePage = blkAfterKey.firstPage;
                                            let (len,newBlk) = try!(writeOverflow(blkAfterKey, &mut *strm, pageManager, fs));
                                            (newBlk, ValueLocation::Overflowed(len,valuePage))
                                        },
                                        Blob::Array(a) => {
                                            if a.len() == 0 {
                                                (blkAfterKey, ValueLocation::Buffer(a))
                                            } else {
                                                let valuePage = blkAfterKey.firstPage;
                                                let strm = a; // TODO need a Read for this
                                                let (len,newBlk) = try!(writeOverflow(blkAfterKey, &mut &*strm, pageManager, fs));
                                                (newBlk, ValueLocation::Overflowed(len,valuePage))
                                            }
                                        },
                                    }
                                } else {
                                    match pair.Value {
                                        Blob::Tombstone => {
                                            (blkAfterKey, ValueLocation::Tombstone)
                                        },
                                        Blob::Stream(ref mut strm) => {
                                            let vread = try!(utils::ReadFully(&mut *strm, &mut vbuf[0 .. maxValueInline+1]));
                                            let vbuf = &vbuf[0 .. vread];
                                            if vread < maxValueInline {
                                                // TODO this alloc+copy is unfortunate
                                                let mut va = Vec::new();
                                                for i in 0 .. vbuf.len() {
                                                    va.push(vbuf[i]);
                                                }
                                                (blkAfterKey, ValueLocation::Buffer(va.into_boxed_slice()))
                                            } else {
                                                let valuePage = blkAfterKey.firstPage;
                                                let (len,newBlk) = try!(writeOverflow(blkAfterKey, &mut (vbuf.chain(strm)), pageManager, fs));
                                                (newBlk, ValueLocation::Overflowed (len,valuePage))
                                            }
                                        },
                                        Blob::Array(a) => {
                                            if a.len() < maxValueInline {
                                                (blkAfterKey, ValueLocation::Buffer(a))
                                            } else {
                                                let valuePage = blkAfterKey.firstPage;
                                                let strm = a; // TODO need a Read for this
                                                let (len,newBlk) = try!(writeOverflow(blkAfterKey, &mut &*strm, pageManager, fs));
                                                (newBlk, ValueLocation::Overflowed(len,valuePage))
                                            }
                                        },
                                    }
                                }
                             },

                             KeyLocation::Overflow(_) => {
                                match pair.Value {
                                    Blob::Tombstone => {
                                        (blkAfterKey, ValueLocation::Tombstone)
                                    },
                                    Blob::Stream(ref mut strm) => {
                                        let valuePage = blkAfterKey.firstPage;
                                        let (len,newBlk) = try!(writeOverflow(blkAfterKey, &mut *strm, pageManager, fs));
                                        (newBlk, ValueLocation::Overflowed(len,valuePage))
                                    },
                                    Blob::Array(a) => {
                                        if a.len() == 0 {
                                            (blkAfterKey, ValueLocation::Buffer(a))
                                        } else {
                                            let valuePage = blkAfterKey.firstPage;
                                            let strm = a; // TODO need a Read for this
                                            let (len,newBlk) = try!(writeOverflow(blkAfterKey, &mut &*strm, pageManager, fs));
                                            (newBlk, ValueLocation::Overflowed(len,valuePage))
                                        }
                                    }
                                }
                             }
                        }
                };

                // whether/not the key/value are to be overflowed is now already decided.
                // now all we have to do is decide if this key/value are going into this leaf
                // or not.  note that it is possible to overflow these and then have them not
                // fit into the current leaf and end up landing in the next leaf.

                st.blk=blkAfterValue;

                // TODO ignore prefixLen for overflowed keys?
                let newPrefixLen = 
                    if st.keys.len()==0 {
                        defaultPrefixLen(&k)
                    } else {
                        bcmp::PrefixMatch(&*st.keys[0].key, &k, st.prefixLen)
                    };
                let sofar = 
                    if newPrefixLen < st.prefixLen {
                        // the prefixLen would change with the addition of this key,
                        // so we need to recalc sofar
                        // TODO is it a problem that we're doing this without List.rev ?
                        let mut sum = 0;
                        for lp in &st.keys {
                            sum = sum + leafPairSize(newPrefixLen, lp);
                        }
                        // TODO iter sum?
                        sum
                    } else {
                        st.sofarLeaf
                    };
                let available = pageSize - (sofar + LEAF_PAGE_OVERHEAD + 1 + newPrefixLen);
                let needed = kLocNeed(&k, &kloc, newPrefixLen) + vLocNeed(&vloc);
                let fit = (available >= needed);
                let writeThisPage = (! st.keys.is_empty()) && (! fit);

                if writeThisPage {
                    writeLeaf(&mut st, false, pb, fs, pageSize, pageManager, &mut *token)
                }

                // TODO ignore prefixLen for overflowed keys?
                let newPrefixLen = 
                    if st.keys.is_empty() {
                        defaultPrefixLen(&k)
                    } else {
                        bcmp::PrefixMatch(&*st.keys[0].key, &k, st.prefixLen)
                    };
                let sofar = 
                    if newPrefixLen < st.prefixLen {
                        // the prefixLen will change with the addition of this key,
                        // so we need to recalc sofar
                        // TODO is it a problem that we're doing this without List.rev ?
                        let mut sum = 0;
                        for lp in &st.keys {
                            sum = sum + leafPairSize(newPrefixLen, lp);
                        }
                        // TODO iter sum?
                        sum
                    } else {
                        st.sofarLeaf
                    };
                let lp = LeafPair {
                            key:k,
                            kLoc:kloc,
                            vLoc:vloc,
                            };

                st.sofarLeaf=sofar + leafPairSize(newPrefixLen, &lp);
                st.keys.push(box lp);
                st.prefixLen=newPrefixLen;
            }

            if !st.keys.is_empty() {
                let isRootNode = st.leaves.is_empty();
                writeLeaf(&mut st, isRootNode, pb, fs, pageSize, pageManager, &mut *token)
            }
            Ok((st.blk,st.leaves,st.firstLeaf))
        }

        fn writeParentNodes<SeekWrite>(startingBlk: PageBlock, 
                                       children: &[pgitem],
                                       pageSize: usize,
                                       fs: &mut SeekWrite,
                                       pageManager: &mut IPages,
                                       token: &mut PendingSegment,
                                       lastLeaf: usize,
                                       firstLeaf: usize,
                                       pb: &mut PageBuilder,
                                      ) -> io::Result<(PageBlock, Vec<pgitem>)> where SeekWrite : Seek+Write {
            // 2 for the page type and flags
            // 2 for the stored count
            // 5 for the extra ptr we will add at the end, a varint, 5 is worst case (page num < 4294967295L)
            // 4 for lastInt32
            const PARENT_PAGE_OVERHEAD :usize = 2 + 2 + 5 + 4;

            fn calcAvailable(currentSize: usize, couldBeRoot: bool, pageSize: usize) -> usize {
                let basicSize = pageSize - currentSize;
                let allowanceForRootNode = if couldBeRoot { size_i32 } else { 0 }; // first/last Leaf, lastInt32 already
                basicSize - allowanceForRootNode
            }

            fn buildParentPage(items: &[&pgitem], 
                               lastPtr: usize, 
                               overflows: &HashMap<usize,usize>,
                               pb : &mut PageBuilder,
                              ) {
                pb.Reset();
                pb.PutByte(PageType::PARENT_NODE as u8);
                pb.PutByte(0u8);
                pb.PutInt16(items.len() as i16);
                // store all the ptrs, n+1 of them
                for x in items.iter() {
                    pb.PutVarint(x.page as u64);
                }
                pb.PutVarint(lastPtr as u64);
                // store all the keys, n of them
                for i in 0 .. items.len() {
                    let x = &items[i];
                    match overflows.get(&i) {
                        Some(pg) => {
                            pb.PutByte(ValueFlag::FLAG_OVERFLOW as u8);
                            pb.PutVarint(x.key.len() as u64);
                            pb.PutInt32(*pg as i32);
                        },
                        None => {
                            pb.PutByte(0u8);
                            pb.PutVarint(x.key.len() as u64);
                            pb.PutArray(&x.key);
                        },
                    }
                }
            }

            fn writeParentPage<SeekWrite>(st: &mut ParentState, 
                                          items: &[&pgitem],
                                          overflows: &HashMap<usize,usize>,
                                          pair:&pgitem, 
                                          isRootNode: bool, 
                                          pb: &mut PageBuilder, 
                                          lastLeaf: usize,
                                          fs: &mut SeekWrite,
                                          pageManager: &mut IPages,
                                          pageSize: usize,
                                          token: &mut PendingSegment,
                                          firstLeaf: usize,
                                         ) where SeekWrite : Seek+Write {
                let pagenum = pair.page;
                // assert st.sofar > 0
                let thisPageNumber = st.blk.firstPage;
                buildParentPage(items, pagenum, &overflows, pb);
                let nextBlk =
                    if isRootNode {
                        pb.SetPageFlag(PageFlag::FLAG_ROOT_NODE as u8);
                        pb.SetSecondToLastInt32(firstLeaf as i32);
                        pb.SetLastInt32(lastLeaf as i32);
                        PageBlock::new(thisPageNumber+1,st.blk.lastPage)
                    } else {
                        if (st.blk.firstPage == st.blk.lastPage) {
                            pb.SetPageFlag(PageFlag::FLAG_BOUNDARY_NODE as u8);
                            let newBlk = pageManager.GetBlock(&mut *token);
                            pb.SetLastInt32(newBlk.firstPage as i32);
                            newBlk
                        } else {
                            PageBlock::new(thisPageNumber+1,st.blk.lastPage)
                        }
                    };
                pb.Write(fs);
                if nextBlk.firstPage != (thisPageNumber+1) {
                    utils::SeekPage(fs, pageSize, nextBlk.firstPage);
                }
                st.sofar = 0;
                st.blk = nextBlk;
                // TODO isn't there a better way to copy a slice?
                let mut ba = Vec::new();
                ba.push_all(&pair.key);
                let pg = pgitem {page:thisPageNumber, key:ba.into_boxed_slice()};
                st.nextGeneration.push(pg);
            }

            // this is the body of writeParentNodes
            let mut st = ParentState {nextGeneration:Vec::new(),sofar:0,blk:startingBlk,};
            let mut items = Vec::new();
            let mut overflows = HashMap::new();
            for i in 0 .. children.len()-1 {
                let pair = &children[i];
                let pagenum = pair.page;

                let neededEitherWay = 1 + Varint::SpaceNeededFor(pair.key.len() as u64) + Varint::SpaceNeededFor(pagenum as u64);
                let neededForInline = neededEitherWay + pair.key.len();
                let neededForOverflow = neededEitherWay + size_i32;
                let couldBeRoot = st.nextGeneration.is_empty();

                let available = calcAvailable(st.sofar, couldBeRoot, pageSize);
                let fitsInline = (available >= neededForInline);
                let wouldFitInlineOnNextPage = ((pageSize - PARENT_PAGE_OVERHEAD) >= neededForInline);
                let fitsOverflow = (available >= neededForOverflow);
                let writeThisPage = (! fitsInline) && (wouldFitInlineOnNextPage || (! fitsOverflow));

                if writeThisPage {
                    // assert sofar > 0
                    writeParentPage(&mut st, &items, &overflows, pair, false, pb, lastLeaf, fs, pageManager, pageSize, &mut *token, firstLeaf);
                }

                if st.sofar == 0 {
                    st.sofar = PARENT_PAGE_OVERHEAD;
                    items.clear();
                }

                items.push(pair);
                if calcAvailable(st.sofar, st.nextGeneration.is_empty(), pageSize) >= neededForInline {
                    st.sofar = st.sofar + neededForInline;
                } else {
                    let keyOverflowFirstPage = st.blk.firstPage;
                    let (_,newBlk) = try!(writeOverflow(st.blk, &mut &*pair.key, pageManager, fs));
                    st.sofar = st.sofar + neededForOverflow;
                    st.blk = newBlk;
                    overflows.insert(items.len()-1,keyOverflowFirstPage);
                }
            }
            let isRootNode = st.nextGeneration.is_empty();
            writeParentPage(&mut st, &items, &overflows, &children[children.len()-1], isRootNode, pb, lastLeaf, fs, pageManager, pageSize, &mut *token, firstLeaf);
            Ok((st.blk,st.nextGeneration))
        }

        // this is the body of Create
        let pageSize = pageManager.PageSize();
        let mut pb = PageBuilder::new(pageSize);
        let mut token = pageManager.Begin();
        let startingBlk = pageManager.GetBlock(&mut token);
        utils::SeekPage(fs, pageSize, startingBlk.firstPage);

        let mut vbuf = vec![0;pageSize].into_boxed_slice();
        let (blkAfterLeaves, leaves, firstLeaf) = try!(writeLeaves(startingBlk, pageManager, source, &mut vbuf, fs, &mut pb, &mut token));

        // all the leaves are written.
        // now write the parent pages.
        // maybe more than one level of them.
        // keep writing until we have written a level which has only one node,
        // which is the root node.

        let lastLeaf = leaves[0].page;

        let rootPage = {
            let mut blk = blkAfterLeaves;
            let mut children = leaves;
            loop {
                let (newBlk,newChildren) = try!(writeParentNodes(blk, &children, pageSize, fs, pageManager, &mut token, lastLeaf, firstLeaf, &mut pb));
                blk = newBlk;
                children = newChildren;
                if children.len()==1 {
                    break;
                }
            }
            children[0].page
        };

        let g = pageManager.End(token, rootPage);
        Ok((g,rootPage))
    }

    use std::io::SeekFrom;
    use std::io::Error;
    use std::io::ErrorKind;
    use std::fs::File;
    use std::fs::OpenOptions;
    use super::SegmentInfo;
    use super::PageReader;
    use super::PageBuffer;
    use std::cmp::min;
    use super::read_i32_be;
    use super::SeekOp;
    use super::ICursor;

    struct myOverflowReadStream {
        fs: File,
        len: usize,
        firstPage: usize,
        buf: Box<[u8]>,
        currentPage: usize,
        sofarOverall: usize,
        sofarThisPage: usize,
        firstPageInBlock: usize,
        offsetToLastPageInThisBlock: usize,
        countRegularDataPagesInBlock: usize,
        boundaryPageNumber: usize,
        bytesOnThisPage: usize,
        offsetOnThisPage: usize,
    }
        
    impl myOverflowReadStream {
        fn new(path: &str, pageSize: usize, _firstPage: usize, _len: usize) -> io::Result<myOverflowReadStream> {
            let f = try!(OpenOptions::new()
                    .read(true)
                    .open(path));
            let mut res = 
                myOverflowReadStream {
                    fs: f,
                    len: _len,
                    firstPage: _firstPage,
                    buf: vec![0;pageSize].into_boxed_slice(),
                    currentPage: _firstPage,
                    sofarOverall: 0,
                    sofarThisPage: 0,
                    firstPageInBlock: 0,
                    offsetToLastPageInThisBlock: 0, // add to firstPageInBlock to get the last one
                    countRegularDataPagesInBlock: 0,
                    boundaryPageNumber: 0,
                    bytesOnThisPage: 0,
                    offsetOnThisPage: 0,
                };
            try!(res.ReadFirstPage());
            Ok(res)
        }

        // TODO consider supporting seek

        fn ReadPage(&mut self) -> io::Result<()> {
            try!(utils::SeekPage(&mut self.fs, self.buf.len(), self.currentPage));
            try!(utils::ReadFully(&mut self.fs, &mut *self.buf));
            // assert PageType is OVERFLOW
            self.sofarThisPage = 0;
            if self.currentPage == self.firstPageInBlock {
                self.bytesOnThisPage = self.buf.len() - (2 + size_i32);
                self.offsetOnThisPage = 2;
            } else if self.currentPage == self.boundaryPageNumber {
                self.bytesOnThisPage = self.buf.len() - size_i32;
                self.offsetOnThisPage = 0;
            } else {
                // assert currentPage > firstPageInBlock
                // assert currentPage < boundaryPageNumber OR boundaryPageNumber = 0
                self.bytesOnThisPage = self.buf.len();
                self.offsetOnThisPage = 0;
            }
            Ok(())
        }

        fn GetLastInt32(&self) -> usize {
            let at = self.buf.len() - size_i32;
            read_i32_be(&self.buf[at .. at+4]) as usize
        }

        fn PageType(&self) -> u8 {
            self.buf[0]
        }

        fn CheckPageFlag(&self, f: u8) -> bool {
            0 != (self.buf[1] & f)
        }

        fn ReadFirstPage(&mut self) -> io::Result<()> {
            self.firstPageInBlock = self.currentPage;
            try!(self.ReadPage());
            if self.PageType() != (PageType::OVERFLOW_NODE as u8) {
                try!(Err(io::Error::new(ErrorKind::InvalidInput, "first overflow page has invalid page type")));
            }
            if self.CheckPageFlag(PageFlag::FLAG_BOUNDARY_NODE) {
                // first page landed on a boundary node
                // lastInt32 is the next page number, which we'll fetch later
                self.boundaryPageNumber = self.currentPage;
                self.offsetToLastPageInThisBlock = 0;
                self.countRegularDataPagesInBlock = 0;
            } else {
                self.offsetToLastPageInThisBlock = self.GetLastInt32();
                if self.CheckPageFlag(PageFlag::FLAG_ENDS_ON_BOUNDARY) {
                    self.boundaryPageNumber = self.currentPage + self.offsetToLastPageInThisBlock;
                    self.countRegularDataPagesInBlock = self.offsetToLastPageInThisBlock - 1;
                } else {
                    self.boundaryPageNumber = 0;
                    self.countRegularDataPagesInBlock = self.offsetToLastPageInThisBlock;
                }
            }
            Ok(())
        }

        fn Read(&mut self, ba: &mut [u8], offset: usize, wanted: usize) -> io::Result<usize> {
            if self.sofarOverall >= self.len {
                Ok(0)
            } else {
                let mut direct = false;
                if (self.sofarThisPage >= self.bytesOnThisPage) {
                    if self.currentPage == self.boundaryPageNumber {
                        self.currentPage = self.GetLastInt32();
                        try!(self.ReadFirstPage());
                    } else {
                        // we need a new page.  and if it's a full data page,
                        // and if wanted is big enough to take all of it, then
                        // we want to read (at least) it directly into the
                        // buffer provided by the caller.  we already know
                        // this candidate page cannot be the first page in a
                        // block.
                        let maybeDataPage = self.currentPage + 1;
                        let isDataPage = 
                            if self.boundaryPageNumber > 0 {
                                ((self.len - self.sofarOverall) >= self.buf.len()) && (self.countRegularDataPagesInBlock > 0) && (maybeDataPage > self.firstPageInBlock) && (maybeDataPage < self.boundaryPageNumber)
                            } else {
                                ((self.len - self.sofarOverall) >= self.buf.len()) && (self.countRegularDataPagesInBlock > 0) && (maybeDataPage > self.firstPageInBlock) && (maybeDataPage <= (self.firstPageInBlock + self.countRegularDataPagesInBlock))
                            };

                        if isDataPage && (wanted >= self.buf.len()) {
                            // assert (currentPage + 1) > firstPageInBlock
                            //
                            // don't increment currentPage here because below, we will
                            // calculate how many pages we actually want to do.
                            direct = true;
                            self.bytesOnThisPage = self.buf.len();
                            self.sofarThisPage = 0;
                            self.offsetOnThisPage = 0;
                        } else {
                            self.currentPage = self.currentPage + 1;
                            try!(self.ReadPage());
                        }
                    }
                }

                if direct {
                    // currentPage has not been incremented yet
                    //
                    // skip the buffer.  note, therefore, that the contents of the
                    // buffer are "invalid" in that they do not correspond to currentPage
                    //
                    let numPagesWanted = wanted / self.buf.len();
                    // assert countRegularDataPagesInBlock > 0
                    let lastDataPageInThisBlock = self.firstPageInBlock + self.countRegularDataPagesInBlock;
                    let theDataPage = self.currentPage + 1;
                    let numPagesAvailable = 
                        if self.boundaryPageNumber>0 { 
                            self.boundaryPageNumber - theDataPage 
                        } else {
                            lastDataPageInThisBlock - theDataPage + 1
                        };
                    let numPagesToFetch = min(numPagesWanted, numPagesAvailable);
                    let bytesToFetch = numPagesToFetch * self.buf.len();
                    // assert bytesToFetch <= wanted

                    try!(utils::SeekPage(&mut self.fs, self.buf.len(), theDataPage));
                    try!(utils::ReadFully(&mut self.fs, &mut ba[offset .. offset + bytesToFetch]));
                    self.sofarOverall = self.sofarOverall + bytesToFetch;
                    self.currentPage = self.currentPage + numPagesToFetch;
                    self.sofarThisPage = self.buf.len();
                    Ok(bytesToFetch)
                } else {
                    let available = min(self.bytesOnThisPage - self.sofarThisPage, self.len - self.sofarOverall);
                    let num = min(available, wanted);
                    for i in 0 .. num {
                        ba[offset+i] = self.buf[self.offsetOnThisPage + self.sofarThisPage + i];
                    }
                    self.sofarOverall = self.sofarOverall + num;
                    self.sofarThisPage = self.sofarThisPage + num;
                    Ok(num)
                }
            }
        }
    }

    impl Read for myOverflowReadStream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let len = buf.len();
            self.Read(buf, 0, len)
        }
    }

    fn readOverflow(path: &str, pageSize: usize, firstPage: usize, buf: &mut [u8]) -> io::Result<usize> {
        let mut ostrm = try!(myOverflowReadStream::new(path, pageSize, firstPage, buf.len()));
        utils::ReadFully(&mut ostrm, buf)
    }

    struct myCursor {
        path: String,
        fs: File,
        len: u64,
        rootPage: usize,
        pr: PageBuffer,
        // TODO hook
        currentPage: usize,
        leafKeys: Vec<usize>,
        countLeafKeys: usize, // only realloc leafKeys when it's too small, TODO could be u16?
        previousLeaf: usize,
        currentKey: i32, // TODO Option<usize>,
        prefix: Option<Box<[u8]>>,
        firstLeaf: usize,
        lastLeaf: usize,
    }

    use super::seek_len;

    impl myCursor {
        fn new(path: &str, pageSize: usize, rootPage: usize) -> io::Result<myCursor> {
            let mut f = try!(OpenOptions::new()
                    .read(true)
                    .open(path));
            let len = try!(seek_len(&mut f));
            let mut res = myCursor {
                path: String::from_str(path),
                fs: f,
                len: len,
                rootPage: rootPage,
                pr: PageBuffer::new(pageSize),
                currentPage: 0,
                leafKeys: Vec::new(),
                countLeafKeys: 0,
                previousLeaf: 0,
                currentKey: -1, // TODO None
                prefix: None,
                firstLeaf: 0, // temporary
                lastLeaf: 0, // temporary
            };
            if ! try!(res.setCurrentPage(rootPage)) {
                return Err(io::Error::new(ErrorKind::InvalidInput, "failed to read root page"));
            }
            if res.pr.PageType() == PageType::LEAF_NODE {
                res.firstLeaf = rootPage;
                res.lastLeaf = rootPage;
            } else if res.pr.PageType() == PageType::PARENT_NODE {
                if ! res.pr.CheckPageFlag(PageFlag::FLAG_ROOT_NODE) { 
                    return Err(io::Error::new(ErrorKind::InvalidInput, "root page lacks flag"));
                }
                res.firstLeaf = res.pr.GetSecondToLastInt32() as usize;
                res.lastLeaf = res.pr.GetLastInt32() as usize;
            } else {
                return Err(io::Error::new(ErrorKind::InvalidInput, "root page has invalid page type"));
            }
              
            Ok(res)
        }

        fn resetLeaf(&mut self) {
            self.countLeafKeys = 0;
            self.previousLeaf = 0;
            self.currentKey = -1; // TODO None;
            self.prefix = None;
        }

        fn setCurrentPage(&mut self, pagenum:usize) -> io::Result<bool> {
            // TODO consider passing a block list for the segment into this
            // cursor so that the code here can detect if it tries to stray
            // out of bounds.

            // TODO if currentPage = pagenum already...
            self.currentPage = pagenum;
            self.resetLeaf();
            if 0 == self.currentPage { 
                Ok(false)
            } else {
                // refuse to go to a page beyond the end of the stream
                // TODO is this the right place for this check?    
                let pos = (self.currentPage - 1) as u64 * self.pr.PageSize() as u64;
                if pos + self.pr.PageSize() as u64 <= self.len {
                    utils::SeekPage(&mut self.fs, self.pr.PageSize(), self.currentPage);
                    self.pr.Read(&mut self.fs);
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
        }

        fn nextInLeaf(&mut self) -> bool {
            if ((self.currentKey+1) as usize) < self.countLeafKeys {
                self.currentKey = self.currentKey + 1;
                true
            } else {
                false
            }
        }

        fn prevInLeaf(&mut self) -> bool {
            if (self.currentKey > 0) {
                self.currentKey = self.currentKey - 1;
                true
            } else {
                false
            }
        }

        fn skipKey(&self, cur: &mut usize) {
            let kflag = self.pr.GetByte(cur);
            let klen = self.pr.GetVarint(cur) as usize;
            if 0 == (kflag & ValueFlag::FLAG_OVERFLOW) {
                let prefixLen = match self.prefix {
                    Some(ref a) => a.len(),
                    None => 0
                };
                *cur = *cur + (klen - prefixLen);
            } else {
                *cur = *cur + size_i32;
            }
        }

        fn skipValue(&self, cur: &mut usize) {
            let vflag = self.pr.GetByte(cur);
            if 0 != (vflag & ValueFlag::FLAG_TOMBSTONE) { 
                ()
            } else {
                let vlen = self.pr.GetVarint(cur) as usize;
                if 0 != (vflag & ValueFlag::FLAG_OVERFLOW) {
                    *cur = *cur + size_i32;
                }
                else {
                    *cur = *cur + vlen;
                }
            }
        }

        fn readLeaf(&mut self) {
            self.resetLeaf();
            let mut cur = 0;
            if self.pr.GetByte(&mut cur) != PageType::LEAF_NODE {
                panic!("leaf has invalid page type");
            }
            self.pr.GetByte(&mut cur);
            self.previousLeaf = self.pr.GetInt32(&mut cur) as usize;
            let prefixLen = self.pr.GetByte(&mut cur) as usize;
            if prefixLen > 0 {
                let mut a = vec![0;prefixLen].into_boxed_slice();
                self.pr.GetIntoArray(&mut cur, &mut a);
                self.prefix = Some(a);
            } else {
                self.prefix = None;
            }
            self.countLeafKeys = self.pr.GetInt16(&mut cur) as usize;
            // assert countLeafKeys>0
            while self.leafKeys.len() < self.countLeafKeys {
                self.leafKeys.push(0);
            }
            for i in 0 .. self.countLeafKeys {
                self.leafKeys[i] = cur;
                self.skipKey(&mut cur);
                self.skipValue(&mut cur);
            }
        }

        fn keyInLeaf(&self, n: usize) -> io::Result<Box<[u8]>> { 
            let mut cur = self.leafKeys[n];
            let kflag = self.pr.GetByte(&mut cur);
            let klen = self.pr.GetVarint(&mut cur) as usize;
            // TODO consider alloc res array here, once for all cases below
            if 0 == (kflag & ValueFlag::FLAG_OVERFLOW) {
                match self.prefix {
                    Some(ref a) => {
                        let prefixLen = a.len();
                        let mut res = vec![0;klen].into_boxed_slice();
                        for i in 0 .. prefixLen {
                            res[i] = a[i];
                        }
                        self.pr.GetIntoArray(&mut cur, &mut res[prefixLen .. klen]);
                        Ok(res)
                    },
                    None => {
                        let mut res = vec![0;klen].into_boxed_slice();
                        self.pr.GetIntoArray(&mut cur, &mut res);
                        Ok(res)
                    },
                }
            } else {
                let pagenum = self.pr.GetInt32(&mut cur) as usize;
                let mut res = vec![0;klen].into_boxed_slice();
                try!(readOverflow(&self.path, self.pr.PageSize(), pagenum, &mut res));
                Ok(res)
            }
        }

        fn compareKeyInLeaf(&self, n: usize, other: &[u8]) -> io::Result<i32> {
            let mut cur = self.leafKeys[n];
            let kflag = self.pr.GetByte(&mut cur);
            let klen = self.pr.GetVarint(&mut cur) as usize;
            if 0 == (kflag & ValueFlag::FLAG_OVERFLOW) {
                let res = 
                    match self.prefix {
                        Some(ref a) => {
                            self.pr.CompareWithPrefix(cur, a, klen, other)
                        },
                        None => {
                            self.pr.Compare(cur, klen, other)
                        },
                    };
                Ok(res)
            } else {
                // TODO this could be more efficient. we could compare the key
                // in place in the overflow without fetching the entire thing.

                // TODO overflowed keys are not prefixed.  should they be?
                let pagenum = self.pr.GetInt32(&mut cur) as usize;
                let mut k = vec![0;klen].into_boxed_slice();
                try!(readOverflow(&self.path, self.pr.PageSize(), pagenum, &mut k));
                let res = bcmp::Compare(&*k, other);
                Ok(res)
            }
        }

        // TODO I wish this func were not using signed integers
        fn searchLeaf(&mut self, k: &[u8], min:i32, max:i32, sop:SeekOp, le: i32, ge: i32) -> io::Result<i32> {
            if max < min {
                match sop {
                    SeekOp::SEEK_EQ => Ok(-1),
                    SeekOp::SEEK_LE => Ok(le),
                    SeekOp::SEEK_GE => Ok(ge),
                }
            } else {
                let mid = (max + min) / 2;
                // assert mid >= 0
                let cmp = try!(self.compareKeyInLeaf(mid as usize, k));
                if 0 == cmp {
                    Ok(mid)
                } else if cmp<0 {
                    self.searchLeaf(k, (mid+1), max, sop, mid, ge)
                } else {
                    self.searchLeaf(k, min, (mid-1), sop, le, mid)
                }
            }
        }

        fn readParentPage(&mut self) -> io::Result<(Vec<usize>,Vec<Box<[u8]>>)> {
            let mut cur = 0;
            if self.pr.GetByte(&mut cur) != PageType::PARENT_NODE {
                return Err(io::Error::new(ErrorKind::InvalidInput, "parent page has invalid page type"));
            }
            cur = cur + 1; // page flags
            let count = self.pr.GetInt16(&mut cur);
            let mut ptrs = Vec::new();
            let mut keys = Vec::new();
            for i in 0 .. count+1 {
                ptrs.push(self.pr.GetVarint(&mut cur) as usize);
            }
            for i in 0 .. count {
                let kflag = self.pr.GetByte(&mut cur);
                let klen = self.pr.GetVarint(&mut cur) as usize;
                if 0 == (kflag & ValueFlag::FLAG_OVERFLOW) {
                    let mut a = vec![0;klen].into_boxed_slice();
                    self.pr.GetIntoArray(&mut cur, &mut a);
                    keys.push(a);
                } else {
                    let pagenum = self.pr.GetInt32(&mut cur) as usize;
                    let mut k = vec![0;klen].into_boxed_slice();
                    try!(readOverflow(&self.path, self.pr.PageSize(), pagenum, &mut k));
                    keys.push(k);
                }
            }
            Ok((ptrs,keys))
        }

        // this is used when moving forward through the leaf pages.
        // we need to skip any overflows.  when moving backward,
        // this is not necessary, because each leaf has a pointer to
        // the leaf before it.
        fn searchForwardForLeaf(&mut self) -> io::Result<bool> {
            let pt = self.pr.PageType();
            if pt == PageType::LEAF_NODE { 
                Ok(true)
            } else if pt == PageType::PARENT_NODE { 
                // if we bump into a parent node, that means there are
                // no more leaves.
                Ok(false)
            } else {
                let lastInt32 = self.pr.GetLastInt32() as usize;
                //
                // an overflow page has a value in its LastInt32 which
                // is one of two things.
                //
                // if it's a boundary node, it's the page number of the
                // next page in the segment.
                //
                // otherwise, it's the number of pages to skip ahead.
                // this skip might take us to whatever follows this
                // overflow (which could be a leaf or a parent or
                // another overflow), or it might just take us to a
                // boundary page (in the case where the overflow didn't
                // fit).  it doesn't matter.  we just skip ahead.
                //
                if self.pr.CheckPageFlag(PageFlag::FLAG_BOUNDARY_NODE) {
                    if try!(self.setCurrentPage(lastInt32)) {
                        self.searchForwardForLeaf()
                    } else {
                        Ok(false)
                    }
                } else {
                    let lastPage = self.currentPage + lastInt32;
                    let endsOnBoundary = self.pr.CheckPageFlag(PageFlag::FLAG_ENDS_ON_BOUNDARY);
                    if endsOnBoundary {
                        if try!(self.setCurrentPage(lastPage)) {
                            let next = self.pr.GetLastInt32() as usize;
                            if try!(self.setCurrentPage(next)) {
                                self.searchForwardForLeaf()
                            } else {
                                Ok(false)
                            }
                        } else {
                            Ok(false)
                        }
                    } else {
                        if try!(self.setCurrentPage(lastPage + 1)) {
                            self.searchForwardForLeaf()
                        } else {
                            Ok(false)
                        }
                    }
                }
            }
        }

        fn leafIsValid(&self) -> bool {
            let ok = (!self.leafKeys.is_empty()) && (self.countLeafKeys > 0) && (self.currentKey >= 0) && (self.currentKey < (self.countLeafKeys as i32));
            ok
        }

        fn search(&mut self, pg: usize, k: &[u8], sop:SeekOp) -> io::Result<()> {
            if try!(self.setCurrentPage(pg)) {
                if PageType::LEAF_NODE == self.pr.PageType() {
                    self.readLeaf();
                    let tmp_countLeafKeys = self.countLeafKeys;
                    self.currentKey = try!(self.searchLeaf(k, 0, (tmp_countLeafKeys - 1) as i32, sop, -1, -1));
                    if SeekOp::SEEK_EQ != sop {
                        if ! self.leafIsValid() {
                            // if LE or GE failed on a given page, we might need
                            // to look at the next/prev leaf.
                            if SeekOp::SEEK_GE == sop {
                                let nextPage =
                                    if self.pr.CheckPageFlag(PageFlag::FLAG_BOUNDARY_NODE) { self.pr.GetLastInt32() as usize }
                                    else if self.currentPage == self.rootPage { 0 }
                                    else { self.currentPage + 1 };
                                if (try!(self.setCurrentPage(nextPage)) && try!(self.searchForwardForLeaf())) {
                                    self.readLeaf();
                                    self.currentKey = 0;
                                }
                            } else {
                                let tmp_previousLeaf = self.previousLeaf;
                                if 0 == self.previousLeaf {
                                    self.resetLeaf();
                                } else if try!(self.setCurrentPage(tmp_previousLeaf)) {
                                    self.readLeaf();
                                    self.currentKey = (self.countLeafKeys - 1) as i32;
                                }
                            }
                        }
                    }
                } else if PageType::PARENT_NODE == self.pr.PageType() {
                    let (ptrs,keys) = try!(self.readParentPage());
                    let found = searchInParentPage(k, &ptrs, &keys, 0);
                    if 0 == found {
                        return self.search(ptrs[ptrs.len() - 1], k, sop);
                    } else {
                        return self.search(found, k, sop);
                    }
                }
            }
            Ok(())
        }
    }

    // TODO it looks like a static function inside impl can't be recursive
    fn searchInParentPage(k: &[u8], ptrs: &Vec<usize>, keys: &Vec<Box<[u8]>>, i: usize) -> usize {
        // TODO linear search?  really?
        if i < keys.len() {
            let cmp = bcmp::Compare(k, &*keys[i]);
            if cmp>0 {
                searchInParentPage(k, ptrs, keys, i+1)
            } else {
                ptrs[i]
            }
        } else {
            0
        }
    }

    impl Drop for myCursor {
        fn drop(&mut self) {
            // TODO
        }
    }

    impl ICursor for myCursor {
        fn IsValid(&self) -> bool {
            self.leafIsValid()
        }

        fn Seek(&mut self, k:&[u8], sop:SeekOp) {
            let rootPage = self.rootPage;
            self.search(rootPage, k, sop).unwrap()
        }

        fn Key(&self) -> Box<[u8]> {
            let currentKey = self.currentKey as usize;
            self.keyInLeaf(currentKey).unwrap()
        }

        fn Value(&self) -> Blob {
            let currentKey = self.currentKey as usize;
            let mut pos = self.leafKeys[currentKey];

            self.skipKey(&mut pos);

            let vflag = self.pr.GetByte(&mut pos);
            if 0 != (vflag & ValueFlag::FLAG_TOMBSTONE) {
                Blob::Tombstone
            } else {
                let vlen = self.pr.GetVarint(&mut pos) as usize;
                if 0 != (vflag & ValueFlag::FLAG_OVERFLOW) {
                    let pagenum = self.pr.GetInt32(&mut pos) as usize;
                    let strm = myOverflowReadStream::new(&self.path, self.pr.PageSize(), pagenum, vlen).unwrap();
                    Blob::Stream(box strm)
                } else {
                    let mut a = vec![0;vlen].into_boxed_slice();
                    self.pr.GetIntoArray(&mut pos, &mut a);
                    Blob::Array(a)
                }
            }
        }

        fn ValueLength(&self) -> i32 {
            let mut cur = self.leafKeys[self.currentKey as usize];

            self.skipKey(&mut cur);

            let vflag = self.pr.GetByte(&mut cur);
            if 0 != (vflag & ValueFlag::FLAG_TOMBSTONE) { -1 }
            else {
                let vlen = self.pr.GetVarint(&mut cur) as i32;
                vlen
            }
        }

        fn KeyCompare(&self, k:&[u8]) -> i32 {
            let currentKey = self.currentKey as usize;
            self.compareKeyInLeaf(currentKey, k).unwrap()
        }

        fn First(&mut self) {
            let firstLeaf = self.firstLeaf;
            if self.setCurrentPage(firstLeaf).unwrap() {
                self.readLeaf();
                self.currentKey = 0;
            }
        }

        fn Last(&mut self) {
            let lastLeaf = self.lastLeaf;
            if self.setCurrentPage(lastLeaf).unwrap() {
                self.readLeaf();
                self.currentKey = self.countLeafKeys as i32 - 1;
            }
        }

        fn Next(&mut self) {
            if ! self.nextInLeaf() {
                let nextPage =
                    if self.pr.CheckPageFlag(PageFlag::FLAG_BOUNDARY_NODE) { self.pr.GetLastInt32() as usize }
                    else if self.pr.PageType() == PageType::LEAF_NODE {
                        if self.currentPage == self.rootPage { 0 }
                        else { self.currentPage + 1 }
                    } else { 0 }
                ;
                if self.setCurrentPage(nextPage).unwrap() && self.searchForwardForLeaf().unwrap() {
                    self.readLeaf();
                    self.currentKey = 0;
                }
            }
        }

        fn Prev(&mut self) {
            if ! self.prevInLeaf() {
                let previousLeaf = self.previousLeaf;
                if 0 == previousLeaf {
                    self.resetLeaf();
                } else if self.setCurrentPage(previousLeaf).unwrap() {
                    self.readLeaf();
                    self.currentKey = self.countLeafKeys as i32 - 1;
                }
            }
        }

    }
}

/*
[<AbstractClass;Sealed>]
type BTreeSegment =
    static member CreateFromSortedSequence(fs:Stream, pageManager:IPages, source:seq<kvp>) = 
        bt.CreateFromSortedSequenceOfKeyValuePairs (fs, pageManager, source)

    #if not
    static member CreateFromSortedSequence(fs:Stream, pageManager:IPages, pairs:seq<byte[]*Stream>, mess:string) = 
        let source = seq { for t in pairs do yield kvp(fst t,snd t) done }
        bt.CreateFromSortedSequenceOfKeyValuePairs (fs, pageManager, source)
    #endif

    static member SortAndCreate(fs:Stream, pageManager:IPages, pairs:System.Collections.Generic.IDictionary<byte[],Stream>) =
#if not
        let keys:byte[][] = (Array.ofSeq pairs.Keys)
        let sortfunc x y = bcmp.Compare x y
        Array.sortInPlaceWith sortfunc keys
        let sortedSeq = seq { for k in keys do yield kvp(k,pairs.[k]) done }
#else
        // TODO which is faster?  how does linq OrderBy implement sorting
        // of a sequence?
        // http://code.logos.com/blog/2010/04/a_truly_lazy_orderby_in_linq.html
        let s1 = pairs.AsEnumerable()
        let s2 = Seq.map (fun (x:System.Collections.Generic.KeyValuePair<byte[],Stream>) -> kvp(x.Key, if x.Value = null then Blob.Tombstone else x.Value |> Blob.Stream)) s1
        let sortedSeq = s2.OrderBy((fun (x:kvp) -> x.Key), ByteComparer())
#endif
        bt.CreateFromSortedSequenceOfKeyValuePairs (fs, pageManager, sortedSeq)

    static member SortAndCreate(fs:Stream, pageManager:IPages, pairs:System.Collections.Generic.IDictionary<byte[],Blob>) =
#if not
        let keys:byte[][] = (Array.ofSeq pairs.Keys)
        let sortfunc x y = bcmp.Compare x y
        Array.sortInPlaceWith sortfunc keys
        let sortedSeq = seq { for k in keys do yield kvp(k,pairs.[k]) done }
#else
        // TODO which is faster?  how does linq OrderBy implement sorting
        // of a sequence?
        // http://code.logos.com/blog/2010/04/a_truly_lazy_orderby_in_linq.html
        let s1 = pairs.AsEnumerable()
        let sortedSeq = s1.OrderBy((fun (x:kvp) -> x.Key), ByteComparer())
#endif
        bt.CreateFromSortedSequenceOfKeyValuePairs (fs, pageManager, sortedSeq)

    #if not
    static member SortAndCreate(fs:Stream, pageManager:IPages, pairs:Map<byte[],Stream>) =
        let keys:byte[][] = pairs |> Map.toSeq |> Seq.map fst |> Array.ofSeq
        let sortfunc x y = bcmp.Compare x y
        Array.sortInPlaceWith sortfunc keys
        let sortedSeq = seq { for k in keys do yield kvp(k,pairs.[k]) done }
        bt.CreateFromSortedSequenceOfKeyValuePairs (fs, pageManager, sortedSeq)
    #endif

    static member OpenCursor(fs, pageSize:int, rootPage:int, hook:Action<ICursor>) :ICursor =
        bt.OpenCursor(fs,pageSize,rootPage,hook)
*/

use std::collections::HashMap;

struct HeaderData {
    // TODO currentState is an ordered copy of segments.Keys.  eliminate duplication?
    // or add assertions and tests to make sure they never get out of sync?
    currentState: Vec<Guid>,
    segments: HashMap<Guid,SegmentInfo>,
    headerOverflow: Option<PageBlock>,
    changeCounter: u64,
    mergeCounter: u64,
}

struct SimplePageManager {
    pageSize : usize,
    nextPage : usize,
}

mod Database {
    use std::io;
    use std::io::Read;
    use std::io::Seek;
    use std::io::SeekFrom;
    use std::io::Write;
    use std::io::Error;
    use std::io::ErrorKind;
    use std::fs::File;
    use std::fs::OpenOptions;
    use std::collections::HashMap;
    use super::utils;
    use super::SegmentInfo;
    use super::Guid;
    use super::PageReader;
    use super::PageBuilder;
    use super::Varint;
    use super::PageBlock;
    use super::HeaderData;
    use super::DbSettings;
    use super::seek_len;

    const HEADER_SIZE_IN_BYTES: usize = 4096;

    impl PendingSegment {
        fn new() -> PendingSegment {
            PendingSegment {blockList: Vec::new()}
        }

        fn AddBlock(&mut self, b: PageBlock) {
            let len = self.blockList.len();
            if (! (self.blockList.is_empty())) && (b.firstPage == self.blockList[len-1].lastPage+1) {
                // note that by consolidating blocks here, the segment info list will
                // not have information about the fact that the two blocks were
                // originally separate.  that's okay, since all we care about here is
                // keeping track of which pages are used.  but the btree code itself
                // is still treating the last page of the first block as a boundary
                // page, even though its pointer to the next block goes to the very
                // next page, because its page manager happened to give it a block
                // which immediately follows the one it had.
                self.blockList[len-1].lastPage = b.lastPage;
            } else {
                self.blockList.push(b);
            }
        }

        fn End(mut self, lastPage: usize) -> (Guid, Vec<PageBlock>, Option<PageBlock>) {
            let len = self.blockList.len();
            let unused = {
                let givenLastPage = self.blockList[len-1].lastPage;
                if lastPage < givenLastPage {
                    self.blockList[len-1].lastPage = lastPage;
                    Some (PageBlock::new(lastPage+1, givenLastPage))
                } else {
                    None
                }
            };
            // consume self return blockList
            (Guid::NewGuid(), self.blockList, unused)
        }
    }

    impl IPages for super::SimplePageManager {
        fn PageSize(&self) -> usize {
            self.pageSize
        }

        fn Begin(&mut self) -> PendingSegment {
            PendingSegment::new()
        }

        fn GetBlock(&mut self, ps: &mut PendingSegment) -> PageBlock {
            let blk = PageBlock::new(self.nextPage, self.nextPage + 10 - 1);
            self.nextPage = self.nextPage + 10;
            ps.AddBlock(blk);
            blk
        }

        fn End(&mut self, ps:PendingSegment, lastPage:usize) -> Guid {
            let (g,_,_) = ps.End(lastPage);
            g
        }

    }

    fn readHeader<R>(fs:&mut R) -> io::Result<(HeaderData,usize,usize)> where R : Read+Seek {
        // TODO this func assumes we are at the beginning of the file?

        fn read<R>(fs: &mut R) -> io::Result<PageReader> where R : Read {
            let mut pr = PageReader::new(HEADER_SIZE_IN_BYTES);
            let got = try!(pr.Read(fs));
            if got < HEADER_SIZE_IN_BYTES {
                Err(io::Error::new(ErrorKind::InvalidInput, "invalid header"))
            } else {
                Ok(pr)
            }
        }

        fn parse<R>(pr: &mut PageReader, fs:&mut R) -> (HeaderData, usize) where R : Read+Seek {
            fn readSegmentList(pr: &mut PageReader) -> (Vec<Guid>,HashMap<Guid,SegmentInfo>) {
                fn readBlockList(prBlocks: &mut PageReader) -> Vec<PageBlock> {
                    let count = prBlocks.GetVarint() as usize;
                    let mut a = Vec::new();
                    for i in 0 .. count {
                        let firstPage = prBlocks.GetVarint() as usize;
                        let countPages = prBlocks.GetVarint() as usize;
                        // blocks are stored as firstPage/count rather than as
                        // firstPage/lastPage, because the count will always be
                        // smaller as a varint
                        a.push(PageBlock::new(firstPage,firstPage + countPages - 1));
                    }
                    a
                }

                let count = pr.GetVarint() as usize;
                let mut a = Vec::new(); // TODO capacity count
                let mut m = HashMap::new(); // TODO capacity count
                for i in 0 .. count {
                    let mut b = [0;16];
                    pr.GetIntoArray(&mut b);
                    let g = Guid::new(b);
                    a.push(g);
                    let root = pr.GetVarint() as usize;
                    let age = pr.GetVarint() as u32;
                    let blocks = readBlockList(pr);
                    let info = SegmentInfo {root:root,age:age,blocks:blocks};
                    m.insert(g,info);
                }
                (a,m)
            }

            // --------

            let pageSize = pr.GetInt32() as usize;
            let changeCounter = pr.GetVarint();
            let mergeCounter = pr.GetVarint();
            let lenSegmentList = pr.GetVarint() as usize;

            let overflowed = pr.GetByte();
            let (state,segments,blk) = 
                if overflowed != 0u8 {
                    let lenChunk1 = pr.GetInt32() as usize;
                    let lenChunk2 = lenSegmentList - lenChunk1;
                    let firstPageChunk2 = pr.GetInt32() as usize;
                    let extraPages = lenChunk2 / pageSize + if (lenChunk2 % pageSize) != 0 { 1 } else { 0 };
                    let lastPageChunk2 = firstPageChunk2 + extraPages - 1;
                    let mut pr2 = PageReader::new(lenSegmentList);
                    // TODO chain?
                    // copy from chunk1 into pr2
                    pr2.ReadPart(fs, 0, lenChunk1);
                    // now get chunk2 and copy it in as well
                    utils::SeekPage(fs, pageSize, firstPageChunk2);
                    pr2.ReadPart(fs, lenChunk1, lenChunk2);
                    let (state,segments) = readSegmentList(&mut pr2);
                    (state, segments, Some (PageBlock::new(firstPageChunk2, lastPageChunk2)))
                } else {
                    let (state,segments) = readSegmentList(pr);
                    (state,segments,None)
                };


            let hd = 
                HeaderData
                {
                    currentState:state,
                    segments:segments,
                    headerOverflow:blk,
                    changeCounter:changeCounter,
                    mergeCounter:mergeCounter,
                };

            (hd, pageSize)
        }

        fn calcNextPage(pageSize: usize, len: usize) -> usize {
            let numPagesSoFar = if pageSize > len { 1 } else { len / pageSize };
            numPagesSoFar + 1
        }

        // --------

        let len = try!(seek_len(fs));
        if len > 0 {
            fs.seek(SeekFrom::Start(0 as u64));
            let mut pr = try!(read(fs));
            let (h, pageSize) = parse(&mut pr, fs);
            let nextAvailablePage = calcNextPage(pageSize, len as usize);
            Ok((h, pageSize, nextAvailablePage))
        } else {
            //let defaultPageSize = settings.DefaultPageSize;
            let defaultPageSize = 4096; // TODO
            let h = 
                HeaderData
                {
                    segments: HashMap::new(),
                    currentState: Vec::new(),
                    headerOverflow: None,
                    changeCounter: 0,
                    mergeCounter: 0,
                };
            let nextAvailablePage = calcNextPage(defaultPageSize, HEADER_SIZE_IN_BYTES);
            Ok((h, defaultPageSize, nextAvailablePage))
        }

    }

    fn consolidateBlockList(blocks: &mut Vec<PageBlock>) {
        blocks.sort_by(|a,b| a.firstPage.cmp(&b.firstPage));
        loop {
            if blocks.len()==1 {
                break;
            }
            let mut did = false;
            for i in 1 .. blocks.len() {
                if blocks[i-1].lastPage+1 == blocks[i].firstPage {
                    blocks[i-1].lastPage = blocks[i].lastPage;
                    blocks.remove(i);
                    did = true;
                    break;
                }
            }
            if !did {
                break;
            }
        }
    }

    fn invertBlockList(blocks: &Vec<PageBlock>) -> Vec<PageBlock> {
        let len = blocks.len();
        let mut result = Vec::new();
        for i in 0 .. len {
            result.push(blocks[i]);
        }
        result.sort_by(|a,b| a.firstPage.cmp(&b.firstPage));
        for i in 0 .. len-1 {
            result[i].firstPage = result[i].lastPage+1;
            result[i].lastPage = result[i+1].firstPage-1;
        }
        result.remove(len-1);
        result
    }

    fn listAllBlocks(h:&HeaderData, segmentsInWaiting:&HashMap<Guid,SegmentInfo>, pageSize: usize) -> Vec<PageBlock> {
        let headerBlock = PageBlock::new(1, HEADER_SIZE_IN_BYTES / pageSize);
        let mut blocks = Vec::new();

        fn grab(blocks: &mut Vec<PageBlock>, from: &HashMap<Guid,SegmentInfo>) {
            for info in from.values() {
                for b in info.blocks.iter() {
                    blocks.push(*b);
                }
            }
        }

        grab(&mut blocks, &h.segments);
        grab(&mut blocks, segmentsInWaiting);
        blocks.push(headerBlock);
        match h.headerOverflow {
            Some(blk) => blocks.push(blk),
            None => ()
        }
        blocks
    }

    struct db {
        path: String,
        pageSize: usize,
        settings: DbSettings,
        fsMine: File,
        header: HeaderData,
        nextPage: usize,
        segmentsInWaiting: HashMap<Guid,SegmentInfo>,
        freeBlocks: Vec<PageBlock>,
        // TODO cursors
        // TODO pendingMerges
    }

    impl db {
        fn new(path : &str, settings : DbSettings) -> io::Result<db> {

            let mut f = try!(OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open(path));

            let (header,pageSize,firstAvailablePage) = try!(readHeader(&mut f));

            let segmentsInWaiting = HashMap::new();
            let mut blocks = listAllBlocks(&header, &segmentsInWaiting, pageSize);
            consolidateBlockList(&mut blocks);
            let mut freeBlocks = invertBlockList(&blocks);
            freeBlocks.sort_by(|a,b| b.CountPages().cmp(&a.CountPages()));

            let res = db {
                path: String::from_str(path),
                pageSize: pageSize,
                settings: settings, 
                fsMine: f, 
                header: header, 
                nextPage: firstAvailablePage,
                segmentsInWaiting: segmentsInWaiting,
                freeBlocks: freeBlocks,
            };
            Ok(res)
        }

        fn getBlock(&mut self, specificSize: usize) -> PageBlock {
            if specificSize > 0 {
                if self.freeBlocks.is_empty() || specificSize > self.freeBlocks[0].CountPages() {
                    let newBlk = PageBlock::new(self.nextPage, self.nextPage+specificSize-1);
                    self.nextPage = self.nextPage + specificSize;
                    newBlk
                } else {
                    let headBlk = self.freeBlocks[0];
                    if headBlk.CountPages() > specificSize {
                        // trim the block to size
                        let blk2 = PageBlock::new(headBlk.firstPage, headBlk.firstPage+specificSize-1); 
                        self.freeBlocks[0].firstPage = self.freeBlocks[0].firstPage + specificSize;
                        // TODO problem: the list is probably no longer sorted.  is this okay?
                        // is a re-sort of the list really worth it?
                        blk2
                    } else {
                        self.freeBlocks.remove(0);
                        headBlk
                    }
                }
            } else {
                if self.freeBlocks.is_empty() {
                    let size = self.settings.PagesPerBlock;
                    let newBlk = PageBlock::new(self.nextPage, self.nextPage+size-1) ;
                    self.nextPage = self.nextPage + size;
                    newBlk
                } else {
                    let headBlk = self.freeBlocks[0];
                    self.freeBlocks.remove(0);
                    headBlk
                }
            }
        }

        // this code should not be called in a release build.  it helps
        // finds problems by zeroing out pages in blocks that
        // have been freed.
        fn stomp(&self, blocks:Vec<PageBlock>) -> io::Result<()> {
            let bad = vec![0;self.pageSize].into_boxed_slice();
            let mut fs = try!(OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&self.path));
            for b in blocks {
                for x in b.firstPage .. b.lastPage+1 {
                    utils::SeekPage(&mut fs, self.pageSize, x);
                    fs.write(&bad);
                }
            }
            Ok(())
        }

        fn addFreeBlocks(&mut self, blocks:Vec<PageBlock>) {

            // all additions to the freeBlocks list should happen here
            // by calling this function.
            //
            // the list is kept consolidated and sorted by size descending.
            // unfortunately this requires two sorts, and they happen here
            // inside a critical section.  but the benefit is considered
            // worth the trouble.
            
            // TODO it is important that freeBlocks contains no overlaps.
            // add debug-only checks to verify?

            // TODO is there such a thing as a block that is so small we
            // don't want to bother with it?  what about a single-page block?
            // should this be a configurable setting?

            // TODO if the last block of the file is free, consider just
            // moving nextPage back.

            for b in blocks {
                self.freeBlocks.push(b);
            }
            consolidateBlockList(&mut self.freeBlocks);
            self.freeBlocks.sort_by(|a,b| b.CountPages().cmp(&a.CountPages()));
        }

        // a stored segmentinfo for a segment is a single blob of bytes.
        // root page
        // age
        // number of pairs
        // each pair is startBlock,countBlocks
        // all in varints

        fn writeHeader(&mut self, hdr:&mut HeaderData) {
            fn spaceNeededForSegmentInfo(info: &SegmentInfo) -> usize {
                let mut a = 0;
                for t in info.blocks.iter() {
                    a = a + Varint::SpaceNeededFor(t.firstPage as u64);
                    a = a + Varint::SpaceNeededFor(t.CountPages() as u64);
                }
                a = a + Varint::SpaceNeededFor(info.root as u64);
                a = a + Varint::SpaceNeededFor(info.age as u64);
                a = a + Varint::SpaceNeededFor(info.blocks.len() as u64);
                a
            }

            fn spaceForHeader(h:&HeaderData) -> usize {
                let mut a = Varint::SpaceNeededFor(h.currentState.len() as u64);
                // TODO use currentState with a lookup into h.segments instead?
                // should be the same, right?
                for info in h.segments.values() {
                    a = a + spaceNeededForSegmentInfo(&info) + 16;
                }
                a
            }

            fn buildSegmentList(h:&HeaderData) -> PageBuilder {
                let space = spaceForHeader(h);
                let mut pb = PageBuilder::new(space);
                // TODO format version number
                pb.PutVarint(h.currentState.len() as u64);
                for g in h.currentState.iter() {
                    pb.PutArray(&g.ToByteArray());
                    match h.segments.get(&g) {
                        Some(info) => {
                            pb.PutVarint(info.root as u64);
                            pb.PutVarint(info.age as u64);
                            pb.PutVarint(info.blocks.len() as u64);
                            // we store PageBlock as first/count instead of first/last, since the
                            // count will always compress better as a varint.
                            for t in info.blocks.iter() {
                                pb.PutVarint(t.firstPage as u64);
                                pb.PutVarint(t.CountPages() as u64);
                            }
                        },
                        None => panic!() // TODO
                    }
                }
                //if 0 != pb.Available then failwith "not exactly full"
                pb
            }

            let mut pb = PageBuilder::new(HEADER_SIZE_IN_BYTES);
            pb.PutInt32(self.pageSize as i32);

            pb.PutVarint(hdr.changeCounter);
            pb.PutVarint(hdr.mergeCounter);

            let pbSegList = buildSegmentList(hdr);
            let buf = pbSegList.Buffer();
            pb.PutVarint(buf.len() as u64);

            let headerOverflow =
                if (pb.Available() >= (buf.len() + 1)) {
                    pb.PutByte(0u8);
                    pb.PutArray(buf);
                    None
                } else {
                    pb.PutByte(1u8);
                    let fits = pb.Available() - 4 - 4;
                    let extra = buf.len() - fits;
                    let extraPages = extra / self.pageSize + if (extra % self.pageSize) != 0 { 1 } else { 0 };
                    //printfn "extra pages: %d" extraPages
                    let blk = self.getBlock(extraPages);
                    utils::SeekPage(&mut self.fsMine, self.pageSize, blk.firstPage);
                    self.fsMine.write(&buf[fits .. buf.len()]);
                    pb.PutInt32(fits as i32);
                    pb.PutInt32(blk.firstPage as i32);
                    pb.PutArray(&buf[0 .. fits]);
                    Some(blk)
                };

            self.fsMine.seek(SeekFrom::Start(0));
            pb.Write(&mut self.fsMine);
            self.fsMine.flush();
            hdr.headerOverflow = headerOverflow
        }

    }

    use super::IPages;
    use super::PendingSegment;

    impl IPages for db {
        fn PageSize(&self) -> usize {
            self.pageSize
        }

        fn Begin(&mut self) -> PendingSegment {
            PendingSegment::new()
        }

        fn GetBlock(&mut self, ps:&mut PendingSegment) -> PageBlock {
            let blk = self.getBlock(0); // specificSize=0 means we don't care how big of a block we get
            ps.AddBlock(blk);
            blk
        }

        fn End(&mut self, ps:PendingSegment, lastPage:usize) -> Guid {
            let (g,blocks,unused) = ps.End(lastPage);
            let info = SegmentInfo {age:0,blocks:blocks,root:lastPage};
            self.segmentsInWaiting.insert(g,info);
            //printfn "wrote %A: %A" g blocks
            match unused {
                Some(b) => self.addFreeBlocks(vec![b]),
                None => ()
            }
            g
        }

    }

}

// ----------------------------------------------------------------

/*

module bt =

    type private myCursor(_fs:Stream, pageSize:int, _rootPage:int, _hook:Action<ICursor>) =

    let OpenCursor(fs, pageSize:int, rootPage:int, hook:Action<ICursor>) :ICursor =
        new myCursor(fs, pageSize, rootPage, hook) :> ICursor

type Database(_io:IDatabaseFile, _settings:DbSettings) =

    let critSectionCursors = obj()
    let mutable cursors:Map<Guid,ICursor list> = Map.empty

    let getCursor segs g fnFree =
        let seg = Map.find g segs
        let rootPage = seg.root
        let fs = io.OpenForReading()
        let hook (csr:ICursor) =
            fs.Close()
            lock critSectionCursors (fun () -> 
                let cur = Map.find g cursors
                let removed = List.filter (fun x -> not (Object.ReferenceEquals(csr, x))) cur
                // if we are removing the last cursor for a segment, we do need to
                // remove that segment guid from the cursors map, not just leave
                // it there with an empty list.
                if List.isEmpty removed then
                    cursors <- Map.remove g cursors
                    match fnFree with
                    | Some f -> f g seg
                    | None -> ()
                else
                    cursors <- Map.add g removed cursors
            )
            //printfn "done with cursor %O" g 
        let csr = BTreeSegment.OpenCursor(fs, pageSize, rootPage, Action<ICursor>(hook))
        // note that getCursor is (and must be) only called from within
        // lock critSectionCursors
        let cur = match Map.tryFind g cursors with
                   | Some c -> c
                   | None -> []
        cursors <- Map.add g (csr :: cur) cursors
        //printfn "added cursor %O: %A" g seg
        csr

    let checkForGoneSegment g seg =
        if not (Map.containsKey g header.segments) then
            // this segment no longer exists
            //printfn "cursor done, segment %O is gone: %A" g seg
            addFreeBlocks seg.blocks

    let critSectionSegmentsInWaiting = obj()

    let critSectionHeader = obj()

    let critSectionMerging = obj()
    // this keeps track of which segments are currently involved in a merge.
    // a segment can only be in one merge at a time.  in effect, this is a list
    // of merge locks for segments.  segments should be removed from this set
    // after the merge has been committed.
    let mutable merging = Set.empty

    let critSectionPendingMerges = obj()
    // this keeps track of merges which have been written but not
    // yet committed.
    let mutable pendingMerges:Map<Guid,Guid list> = Map.empty

    let tryMerge segs =
        let requestMerge () =
            lock critSectionMerging (fun () ->
                let want = Set.ofSeq segs
                let already = Set.intersect want merging
                if Set.isEmpty already then
                    merging <- Set.union merging want
                    true
                else
                    false
            )

        let merge () = 
            // TODO this is silly if segs has only one item in it
            //printfn "merge getting cursors: %A" segs
            let clist = lock critSectionCursors (fun () ->
                let h = header
                List.map (fun g -> getCursor h.segments g (Some checkForGoneSegment)) segs
            )
            use mc = MultiCursor.Create clist
            let pairs = CursorUtils.ToSortedSequenceOfKeyValuePairs mc
            use fs = io.OpenForWriting()
            let (g,_) = BTreeSegment.CreateFromSortedSequence(fs, pageManager, pairs)
            //printfn "merged %A to get %A" segs g
            g

        let storePendingMerge g =
            lock critSectionPendingMerges (fun () ->
                pendingMerges <- Map.add g segs pendingMerges
                // TODO assert segs are in merging set?
            )

        if requestMerge () then
            //printfn "requestMerge Some"
            let later() = 
                //printfn "inside later"
                let g = merge ()
                storePendingMerge g
                g
            Some later
        else
            //printfn "requestMerge None"
            None

    let removePendingMerge g =
        let doneMerge segs =
            lock critSectionMerging (fun () ->
                let removing = Set.ofSeq segs
                // TODO assert is subset?
                merging <- Set.difference merging removing
            )

        let segs = Map.find g pendingMerges
        doneMerge segs
        lock critSectionPendingMerges (fun () ->
            pendingMerges <- Map.remove g pendingMerges
        )

    // only call this if you have the writeLock
    let commitMerge (newGuid:Guid) =
        // TODO we could check to see if this guid is already in the list.

        let lstOld = Map.find newGuid pendingMerges
        let countOld = List.length lstOld                                         
        let oldGuidsAsSet = List.fold (fun acc g -> Set.add g acc) Set.empty lstOld
        let lstAges = List.map (fun g -> (Map.find g header.segments).age) lstOld
        let age = 1 + List.max lstAges

        let segmentsBeingReplaced = Set.fold (fun acc g -> Map.add g (Map.find g header.segments) acc ) Map.empty oldGuidsAsSet

        let oldHeaderOverflow = lock critSectionHeader (fun () -> 
            let ndxFirstOld = List.findIndex (fun g -> g=List.head lstOld) header.currentState
            let subListOld = List.skip ndxFirstOld header.currentState |> List.take countOld
            // if the next line fails, it probably means that somebody tried to merge a set
            // of segments that are not contiguous in currentState.
            if lstOld <> subListOld then failwith (sprintf "segments not found: lstOld = %A  currentState = %A" lstOld header.currentState)
            let before = List.take ndxFirstOld header.currentState
            let after = List.skip (ndxFirstOld + countOld) header.currentState
            let newState = before @ (newGuid :: after)
            let segmentsWithoutOld = Map.filter (fun g _ -> not (Set.contains g oldGuidsAsSet)) header.segments
            let newSegmentInfo = Map.find newGuid segmentsInWaiting
            let newSegments = Map.add newGuid {newSegmentInfo with age=age} segmentsWithoutOld
            let newHeaderBeforeWriting = {
                changeCounter=header.changeCounter
                mergeCounter=header.mergeCounter + 1L
                currentState=newState 
                segments=newSegments
                headerOverflow=None
                }
            let newHeader = writeHeader newHeaderBeforeWriting
            let oldHeaderOverflow = header.headerOverflow
            header <- newHeader
            oldHeaderOverflow
        )
        removePendingMerge newGuid
        // the segment we just committed can now be removed from
        // the segments in waiting list
        lock critSectionSegmentsInWaiting (fun () ->
            segmentsInWaiting <- Map.remove newGuid segmentsInWaiting
        )
        //printfn "segmentsBeingReplaced: %A" segmentsBeingReplaced
        // don't free blocks from any segment which still has a cursor
        lock critSectionCursors (fun () -> 
            let segmentsToBeFreed = Map.filter (fun g _ -> not (Map.containsKey g cursors)) segmentsBeingReplaced
            //printfn "oldGuidsAsSet: %A" oldGuidsAsSet
            let blocksToBeFreed = Seq.fold (fun acc info -> info.blocks @ acc) List.empty (Map.values segmentsToBeFreed)
            match oldHeaderOverflow with
            | Some blk ->
                let blocksToBeFreed = PageBlock(blk.firstPage, blk.lastPage) :: blocksToBeFreed
                addFreeBlocks blocksToBeFreed
            | None ->
                addFreeBlocks blocksToBeFreed
        )
        // note that we intentionally do not release the writeLock here.
        // you can change the segment list more than once while holding
        // the writeLock.  the writeLock gets released when you Dispose() it.

    // only call this if you have the writeLock
    let commitSegments (newGuids:seq<Guid>) fnHook =
        // TODO we could check to see if this guid is already in the list.

        let newGuidsAsSet = Seq.fold (fun acc g -> Set.add g acc) Set.empty newGuids

        let mySegmentsInWaiting = Map.filter (fun g _ -> Set.contains g newGuidsAsSet) segmentsInWaiting
        //printfn "committing: %A" mySegmentsInWaiting
        let oldHeaderOverflow = lock critSectionHeader (fun () -> 
            let newState = (List.ofSeq newGuids) @ header.currentState
            let newSegments = Map.fold (fun acc g info -> Map.add g {info with age=0} acc) header.segments mySegmentsInWaiting
            let newHeaderBeforeWriting = {
                changeCounter=header.changeCounter + 1L
                mergeCounter=header.mergeCounter
                currentState=newState
                segments=newSegments
                headerOverflow=None
                }
            let newHeader = writeHeader newHeaderBeforeWriting
            let oldHeaderOverflow = header.headerOverflow
            header <- newHeader
            oldHeaderOverflow
        )
        //printfn "after commit, currentState: %A" header.currentState
        //printfn "after commit, segments: %A" header.segments
        // all the segments we just committed can now be removed from
        // the segments in waiting list
        lock critSectionSegmentsInWaiting (fun () ->
            let remainingSegmentsInWaiting = Map.filter (fun g _ -> Set.contains g newGuidsAsSet |> not) segmentsInWaiting
            segmentsInWaiting <- remainingSegmentsInWaiting
        )
        match oldHeaderOverflow with
        | Some blk -> addFreeBlocks [ PageBlock(blk.firstPage, blk.lastPage) ]
        | None -> ()
        // note that we intentionally do not release the writeLock here.
        // you can change the segment list more than once while holding
        // the writeLock.  the writeLock gets released when you Dispose() it.

        match fnHook with
        | Some f -> f()
        | None -> ()

    let critSectionInTransaction = obj()
    let mutable inTransaction = false 
    let mutable waiting = Deque.empty

    let getWriteLock front timeout fnCommitSegmentsHook =
        let whence = Environment.StackTrace // TODO remove this.  it was just for debugging.
        let createWriteLockObject () =
            let isReleased = ref false
            let release() =
                isReleased := true
                let next = lock critSectionInTransaction (fun () ->
                    if Deque.isEmpty waiting then
                        //printfn "nobody waiting. tx done"
                        inTransaction <- false
                        None
                    else
                        //printfn "queue has %d waiting.  next." (Queue.length waiting)
                        let f = Deque.head waiting
                        waiting <- Deque.tail waiting
                        //printfn "giving writeLock to next"
                        Some f
                )
                match next with
                | Some f ->
                    f()
                    //printfn "done giving writeLock to next"
                | None -> ()
            {
            new System.Object() with
                override this.Finalize() =
                    let already = !isReleased
                    if not already then failwith (sprintf "a writelock must be explicitly disposed: %s" whence)

            interface IWriteLock with
                member this.Dispose() =
                    let already = !isReleased
                    if already then failwith "only dispose a writelock once"
                    release()
                    GC.SuppressFinalize(this)

                member this.CommitMerge(g:Guid) =
                    let already = !isReleased
                    if already then failwith "don't use a writelock after you dispose it"
                    commitMerge g
                    // note that we intentionally do not release the writeLock here.
                    // you can change the segment list more than once while holding
                    // the writeLock.  the writeLock gets released when you Dispose() it.

                member this.CommitSegments(newGuids:seq<Guid>) =
                    let already = !isReleased
                    if already then failwith "don't use a writelock after you dispose it"
                    commitSegments newGuids fnCommitSegmentsHook
                    // note that we intentionally do not release the writeLock here.
                    // you can change the segment list more than once while holding
                    // the writeLock.  the writeLock gets released when you Dispose() it.
            }

        lock critSectionInTransaction (fun () -> 
            if inTransaction then 
                let ev = new System.Threading.ManualResetEventSlim()
                let cb () = ev.Set()
                if front then
                    waiting <- Deque.cons cb waiting
                else
                    waiting <- Deque.conj cb waiting
                //printfn "Add to wait list: %O" whence
                async {
                    let! b = Async.AwaitWaitHandle(ev.WaitHandle, timeout)
                    ev.Dispose()
                    if b then
                        let lck = createWriteLockObject () 
                        return lck
                    else
                        return failwith "timeout waiting for write lock"
                }
            else 
                //printfn "No waiting: %O" whence
                inTransaction <- true
                async { 
                    let lck = createWriteLockObject () 
                    return lck
                }
            )

    let getPossibleMerge level min all =
        let h = header
        let segmentsOfAge = List.filter (fun g -> (Map.find g h.segments).age=level) h.currentState
        // TODO it would be nice to be able to have more than one merge happening in a level

        // TODO we are trusting segmentsOfAge to be contiguous.  need test cases to
        // verify that currentState always ends up with monotonically increasing age.
        let count = List.length segmentsOfAge
        if count > min then 
            //printfn "NEED MERGE %d -- %d" level count
            // (List.skip) we always merge the stuff at the end of the level so things
            // don't get split up when more segments get prepended to the
            // beginning.
            // TODO if we only do partial here, we might want to schedule a job to do more.
            let grp = if all then segmentsOfAge else List.skip (count - min) segmentsOfAge
            tryMerge grp
        else
            //printfn "no merge needed %d -- %d" level count
            None

    let wrapMergeForLater f = async {
        let g = f()
        //printfn "now waiting for writeLock"
        // merges go to the front of the queue
        use! tx = getWriteLock true (-1) None
        tx.CommitMerge g
        return [ g ]
    }

    let critSectionBackgroundMergeJobs = obj()
    let mutable backgroundMergeJobs = List.empty

    let startBackgroundMergeJob f =
        //printfn "starting background job"
        // TODO this is starving.
        async {
            //printfn "inside start background job"
            let! completor = Async.StartChild f
            lock critSectionBackgroundMergeJobs (fun () -> 
                backgroundMergeJobs <- completor :: backgroundMergeJobs 
                )
            //printfn "inside start background job step 2"
            let! result = completor
            //printfn "inside start background job step 3"
            ignore result
            lock critSectionBackgroundMergeJobs (fun () -> 
                backgroundMergeJobs <- List.filter (fun x -> not (Object.ReferenceEquals(x,completor))) backgroundMergeJobs
                )
        } |> Async.Start

    let doAutoMerge() = 
        if settings.AutoMergeEnabled then
            for level in 0 .. 3 do // TODO max merge level immediate
                match getPossibleMerge level settings.AutoMergeMinimumPages false with
                | Some f -> 
                    let g = f()
                    commitMerge g
                | None -> 
                    () // printfn "cannot merge level %d" level
            for level in 4 .. 7 do // TODO max merge level
                match getPossibleMerge level settings.AutoMergeMinimumPages false with
                | Some f -> 
                    f |> wrapMergeForLater |> startBackgroundMergeJob
                | None -> 
                    () // printfn "cannot merge level %d" level

    let dispose itIsSafeToAlsoFreeManagedObjects =
        //let blocks = consolidateBlockList header
        //printfn "%A" blocks
        if itIsSafeToAlsoFreeManagedObjects then
            // we don't want to close fsMine until all background jobs
            // are completed.
            let bg = backgroundMergeJobs
            if not (List.isEmpty bg) then
                bg |> Async.Parallel |> Async.RunSynchronously |> ignore

            fsMine.Close()

    static member DefaultSettings = 
        {
            AutoMergeEnabled = true
            AutoMergeMinimumPages = 4
            DefaultPageSize = 4096
            PagesPerBlock = 256
        }

    new(_io:IDatabaseFile) =
        new Database(_io, Database.DefaultSettings)

    override this.Finalize() =
        dispose false

    interface IDatabase with
        member this.Dispose() =
            dispose true
            // TODO what happens if there are open cursors?
            // we could throw.  but why?  maybe we should just
            // let them live until they're done.  does the db
            // object care?  this would be more tricky if we were
            // pooling and reusing read streams.  similar issues
            // for background writes as well.
            GC.SuppressFinalize(this)

        member this.WriteSegmentFromSortedSequence(pairs:seq<kvp>) =
            use fs = io.OpenForWriting()
            let (g,_) = BTreeSegment.CreateFromSortedSequence(fs, pageManager, pairs)
            g

        member this.WriteSegment(pairs:System.Collections.Generic.IDictionary<byte[],Stream>) =
            use fs = io.OpenForWriting()
            let (g,_) = BTreeSegment.SortAndCreate(fs, pageManager, pairs)
            g

        member this.WriteSegment(pairs:System.Collections.Generic.IDictionary<byte[],Blob>) =
            use fs = io.OpenForWriting()
            let (g,_) = BTreeSegment.SortAndCreate(fs, pageManager, pairs)
            g

        member this.Merge(level:int, howMany:int, all:bool) =
            let maybe = getPossibleMerge level howMany all
            match maybe with
            | Some f ->
                let blk = wrapMergeForLater f
                Some blk
            | None -> 
                None

        member this.BackgroundMergeJobs() = 
            backgroundMergeJobs

        member this.ForgetWaitingSegments(guids:seq<Guid>) =
            // TODO need a test case for this
            let guidsAsSet = Seq.fold (fun acc g -> Set.add g acc) Set.empty guids
            let mySegmentsInWaiting = Map.filter (fun g _ -> Set.contains g guidsAsSet) segmentsInWaiting
            lock critSectionSegmentsInWaiting (fun () ->
                let remainingSegmentsInWaiting = Map.filter (fun g _ -> Set.contains g guidsAsSet |> not) segmentsInWaiting
                segmentsInWaiting <- remainingSegmentsInWaiting
            )
            lock critSectionCursors (fun () -> 
                let segmentsToBeFreed = Map.filter (fun g _ -> not (Map.containsKey g cursors)) mySegmentsInWaiting
                let blocksToBeFreed = Seq.fold (fun acc info -> info.blocks @ acc) List.empty (Map.values segmentsToBeFreed)
                addFreeBlocks blocksToBeFreed
            )

        member this.OpenCursor() =
            // TODO this cursor needs to expose the changeCounter and segment list
            // on which it is based. for optimistic writes. caller can grab a cursor,
            // do their writes, then grab the writelock, and grab another cursor, then
            // compare the two cursors to see if anything important changed.  if not,
            // commit their writes.  if so, nevermind the written segments and start over.

            // TODO we also need a way to open a cursor on segments in waiting
            let clist = lock critSectionCursors (fun () ->
                let h = header
                List.map (fun g -> getCursor h.segments g (Some checkForGoneSegment)) h.currentState
            )
            let mc = MultiCursor.Create clist
            LivingCursor.Create mc

        member this.OpenSegmentCursor(g:Guid) =
            let csr = lock critSectionCursors (fun () ->
                let h = header
                getCursor h.segments g (Some checkForGoneSegment)
            )
            csr

        member this.GetFreeBlocks() = freeBlocks

        member this.PageSize() = pageSize

        member this.ListSegments() =
            (header.currentState, header.segments)

        member this.RequestWriteLock(timeout:int) =
            // TODO need a test case for this
            getWriteLock false timeout (Some doAutoMerge)

        member this.RequestWriteLock() =
            getWriteLock false (-1) (Some doAutoMerge)

    type PairBuffer(_db:IDatabase, _limit:int) =
        let db = _db
        let limit = _limit
        let d = System.Collections.Generic.Dictionary<byte[],Blob>()
        let mutable segs = []
        let emptyByteArray:byte[] = Array.empty
        let emptyBlobValue = Blob.Array emptyByteArray

        member this.Flush() =
            if d.Count > 0 then
                let g = db.WriteSegment(d)
                segs <- g :: segs
                d.Clear()

        member this.AddPair(k:byte[], v:Blob) =
            // TODO dictionary deals with byte[] keys by reference.
            d.[k] <- v
            if d.Count >= limit then
                this.Flush()

        member this.AddEmptyKey(k:byte[]) =
            this.AddPair(k, emptyBlobValue)

        member this.Commit(tx:IWriteLock) =
            tx.CommitSegments segs
            segs <- []
*/

struct foo {
    num : usize,
    i : usize,
}

impl Iterator for foo {
    type Item = kvp;
    // TODO this doesn't actually generate the pairs in order
    fn next(& mut self) -> Option<kvp> {
        if self.i >= self.num {
            None
        }
        else {
            fn create_array(n : usize) -> Box<[u8]> {
                let mut kv = Vec::new();
                for i in 0 .. n {
                    kv.push(i as u8);
                }
                let k = kv.into_boxed_slice();
                k
            }

            let k = format!("{}", self.i).into_bytes().into_boxed_slice();
            let v = format!("{}", self.i * 2).into_bytes().into_boxed_slice();
            let r = kvp{Key:k, Value:Blob::Array(v)};
            self.i = self.i + 1;
            Some(r)
        }
    }
}

fn hack() -> io::Result<bool> {
    use std::fs::File;

    let mut f = try!(File::create("data.bin"));

    let src = foo {num:100, i:0};
    let mut mgr = SimplePageManager {pageSize: 4096, nextPage: 1};
    bt::CreateFromSortedSequenceOfKeyValuePairs(&mut f, &mut mgr, src);

    let res : io::Result<bool> = Ok(true);
    res
}

fn main() {
    hack();
}

// derive debug is %A
// [u8] is not on the heap.  it's like a primitive that is 7 bytes long.  it's a value type.
// no each()
// no exceptions.  use Result<>
// no currying, no partial application
// no significant whitespace
// strings are utf8, no converting things
// seriously miss full type inference
// weird that it's safe to use unsigned
// don't avoid mutability.  in rust, it's safe, and avoiding it is painful.
// braces vim %
// semicolons A ;, end-of-line comments
// typing tetris
// miss sprintf syntax
// Read,Write,Seek traits so much better design than .NET streams
//
//
// casts.  are casts between i32 and usize safe?  are they checked?
//
// too much usize.  use a more specific, smaller unsigned type.
//
// why do usize and u64 require a cast?
//
// the write_i32_etc routines should all just use unsigned
//
