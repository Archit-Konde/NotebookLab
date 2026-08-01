-- Name: 007_chunk_positions.sql
-- Purpose: Repair chunk ordering for documents imported before v0.8.1.
-- Description: Chunk positions were numbered from zero for each page, because
--   the chunker was called once per page and numbers what it is given. A
--   ten-page document therefore held ten chunks numbered 0, ten numbered 1, and
--   so on. Every reader orders by position, so the pages interleaved and a
--   transform assembled the first chunk of every page, then the second chunk of
--   every page, instead of the document in reading order.
--
--   The ingestion path is fixed, but documents already imported keep the old
--   numbering, and nothing about the app tells the user their older imports read
--   out of order. This repairs them in place rather than leaving that as
--   homework.
--
--   Ordering is by page first, then the old position within that page, then
--   rowid to break any remaining tie deterministically. rowid is insertion
--   order, which for chunks of the same page is the order they were written.
-- Tech Stack: SQLite
-- License: MIT
-- Authors: Amey Thakur (https://github.com/Amey-Thakur)
--          Archit Konde (https://github.com/Archit-Konde)
-- Date: 2026-08-01

/* The new numbering is worked out in full before a single row changes.

   Computing it inside the UPDATE instead looks tidier and is wrong: SQLite
   re-evaluates a correlated subquery per row against the table the statement is
   already rewriting, so the window function reads positions it has itself just
   changed and the numbering collapses into repeats. A snapshot in a temporary
   table is the only ordering the UPDATE can trust. */
CREATE TEMP TABLE chunk_renumber AS
SELECT
    rowid AS chunk_rowid,
    ROW_NUMBER() OVER (
        PARTITION BY document_id
        ORDER BY
            /* A chunk with no page sorts first rather than last, which is where
               plain text and Markdown live: one page, page_number left null. */
            COALESCE(page_number, -1),
            position,
            /* rowid is insertion order, so chunks written by the same call keep
               the order the chunker emitted them in. */
            rowid
    ) - 1 AS seq
FROM chunks
WHERE document_id IN (
    /* Only documents that actually carry duplicate positions, which is the
       signature of the bug. Rewriting an entire library to change nothing would
       be a slow first launch for no benefit. */
    SELECT document_id
    FROM chunks
    GROUP BY document_id, position
    HAVING COUNT(*) > 1
);

UPDATE chunks
SET position = (
    SELECT seq FROM chunk_renumber WHERE chunk_rowid = chunks.rowid
)
WHERE rowid IN (SELECT chunk_rowid FROM chunk_renumber);

DROP TABLE chunk_renumber;
