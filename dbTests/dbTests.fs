﻿
module fsTests

open System
open System.Collections.Generic
open System.IO
open Xunit

open Zumero.LSM
open Zumero.LSM.fs

let tid() = 
    let g = Guid.NewGuid().ToString()
    let g2 = g.Replace ("{", "")
    let g3 = g2.Replace ("}", "")
    let g4 = g3.Replace ("-", "")
    g4

type dseg = Dictionary<byte[],Stream>

let to_utf8 (s:string) =
    System.Text.Encoding.UTF8.GetBytes (s)

let from_utf8 (ba:byte[]) =
    System.Text.Encoding.UTF8.GetString (ba, 0, ba.Length)

let insert (ds:dseg) (sk:string) (sv:string) =
    let k = to_utf8 sk
    let v = new MemoryStream(System.Text.Encoding.UTF8.GetBytes (sv))
    ds.[k] <- v

[<Fact>]
let empty_cursor() = 
    let f = dbf("empty_cursor" + tid())
    use db = new Database(f) :> IDatabase
    use csr = db.OpenCursor()
    csr.First ()
    Assert.False (csr.IsValid ())
    csr.Last ()
    Assert.False (csr.IsValid ())

[<Fact>]
let simple_write() = 
    let f = dbf("simple_write" + tid())
    use db = new Database(f) :> IDatabase
    let d = dseg()
    for i in 1 .. 100 do
        let s = i.ToString()
        insert d s s
    let seg = db.WriteSegment d
    async {
        use! tx = db.RequestWriteLock()
        tx.CommitSegments [ seg ]
    } |> Async.RunSynchronously
    use csr = db.OpenCursor()
    csr.Seek ((42).ToString() |> to_utf8, SeekOp.SEEK_EQ)
    Assert.True (csr.IsValid())
    csr.Next()
    let k = csr.Key() |> from_utf8
    Assert.Equal<string> ("43", k)

[<Fact>]
let multiple() = 
    let f = dbf("multiple" + tid())
    use db = new Database(f) :> IDatabase
    let NUM = 10
    let rand = Random()

    let createMemorySegment (rand:Random) count =
        let d = Dictionary<byte[],Stream>()
        for q in 1 .. count do
            let sk = rand.Next().ToString()
            let sv = rand.Next().ToString()
            insert d sk sv
        d

    let start i = async {
        let commit g = async {
            use! tx = db.RequestWriteLock()
            tx.CommitSegments (g :: List.empty)
            if i%4=0 then
                match db.MergeAll() with
                | Some f ->
                    let blk = async {
                        let! g = f
                        tx.CommitMerge g
                    }
                    do! blk
                | None -> ()
            }

        let count = rand.Next(10000)
        let d = createMemorySegment rand count
        let g = db.WriteSegment(d)
        do! commit g
    }

    let c = seq { for i in 0 .. NUM-1 do yield i; done }
    let workers = Seq.fold (fun acc i -> (start i) :: acc) List.empty c
    let go = Async.Parallel workers
    Async.RunSynchronously go |> ignore

    match db.MergeAll() with
    | Some f ->
        async {
            let! g = f
            use! tx = db.RequestWriteLock()
            tx.CommitMerge g
        } |> Async.RunSynchronously
    | None -> ()

    let loop() = 
        use csr = db.OpenCursor()
        csr.First()
        let mutable count = 0
        while csr.IsValid() do
            count <- count + 1
            csr.Next()

    loop()

[<Fact>]
let lexographic() = 
    let f = dbf("lexographic" + tid())
    use db = new Database(f) :> IDatabase
    let d = dseg()
    insert d "8" ""
    insert d "10" ""
    insert d "20" ""
    let g = db.WriteSegment(d)
    async {
        use! tx = db.RequestWriteLock()
        tx.CommitSegments [ g ]
    } |> Async.RunSynchronously

    use csr = db.OpenCursor()
    csr.First()
    Assert.True(csr.IsValid())
    Assert.Equal<string> ("10", csr.Key () |> from_utf8)

    csr.Next()
    Assert.True(csr.IsValid())
    Assert.Equal<string> ("20", csr.Key () |> from_utf8)

    csr.Next()
    Assert.True(csr.IsValid())
    Assert.Equal<string> ("8", csr.Key () |> from_utf8)

    csr.Next()
    Assert.False(csr.IsValid())

    // --------
    csr.Last()
    Assert.True(csr.IsValid())
    Assert.Equal<string> ("8", csr.Key () |> from_utf8)

    csr.Prev()
    Assert.True(csr.IsValid())
    Assert.Equal<string> ("20", csr.Key () |> from_utf8)

    csr.Prev()
    Assert.True(csr.IsValid())
    Assert.Equal<string> ("10", csr.Key () |> from_utf8)

    csr.Prev()
    Assert.False(csr.IsValid())

[<Fact>]
let weird() = 
    let f = dbf("weird" + tid())
    use db = new Database(f) :> IDatabase
    let t1 = dseg()
    for i in 0 .. 100-1 do
        let sk = i.ToString("000")
        let sv = i.ToString()
        insert t1 sk sv
    let t2 = dseg()
    for i in 0 .. 1000-1 do
        let sk = i.ToString("00000")
        let sv = i.ToString()
        insert t2 sk sv
    let g1 = db.WriteSegment t1
    let g2 = db.WriteSegment t2
    async {
        use! tx = db.RequestWriteLock()
        tx.CommitSegments [ g1 ]
    } |> Async.RunSynchronously
    async {
        use! tx = db.RequestWriteLock()
        tx.CommitSegments [ g2 ]
    } |> Async.RunSynchronously
    use csr = db.OpenCursor()
    csr.First()
    for i in 0 .. 100-1 do
        csr.Next()
        Assert.True(csr.IsValid())
    for i in 0 .. 50-1 do
        csr.Prev()
        Assert.True(csr.IsValid())
    for i in 0 .. 100-1 do
        csr.Next()
        Assert.True(csr.IsValid())
        csr.Next()
        Assert.True(csr.IsValid())
        csr.Prev()
        Assert.True(csr.IsValid())
    for i in 0 .. 50-1 do
        csr.Seek(csr.Key(), SeekOp.SEEK_EQ);
        Assert.True(csr.IsValid())
        csr.Next()
        Assert.True(csr.IsValid())
    for i in 0 .. 50-1 do
        csr.Seek(csr.Key(), SeekOp.SEEK_EQ);
        Assert.True(csr.IsValid())
        csr.Prev()
        Assert.True(csr.IsValid())
    for i in 0 .. 50-1 do
        csr.Seek(csr.Key(), SeekOp.SEEK_LE);
        Assert.True(csr.IsValid())
        csr.Prev()
        Assert.True(csr.IsValid())
    for i in 0 .. 50-1 do
        csr.Seek(csr.Key(), SeekOp.SEEK_GE);
        Assert.True(csr.IsValid())
        csr.Next()
        Assert.True(csr.IsValid())
    let s = csr.Key() |> from_utf8
    // got the following value from the debugger.
    // just want to make sure that it doesn't change
    // and all combos give the same answer.
    Assert.Equal<string>("00148", s); 

