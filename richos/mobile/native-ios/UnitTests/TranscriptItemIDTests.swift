import Foundation
import Testing
import UIKit
@testable import RichOSNative

/// The list's ids are unique, whatever order the rows come in. The list's diffable data source
/// throws on a repeated identifier and that ends the app: on the test iPhone (2026-10-21,
/// RichOSNative-2026-10-21-144802.ips) it closed six seconds after Send, in
/// `TranscriptView.Coordinator.apply` -> `appendItemsWithIdentifiers:`. The rows are in the Mac's
/// cursor order with the phone's unsent bubbles after them, so a day can come back below a later one.
@Suite("Transcript item ids")
@MainActor
struct TranscriptItemIDTests {
    static let calendar: Calendar = {
        var c = Calendar(identifier: .gregorian)
        c.timeZone = TimeZone(identifier: "America/New_York")!
        return c
    }()
    /// 2026-10-21 18:00 UTC (2 PM in New York).
    static let now = Date(timeIntervalSince1970: 1_792_605_600)
    static let dayMs: Int64 = 86_400_000

    static func row(_ id: String, _ author: ScreenModel.Row.Author, at ms: Int64, line: String? = nil) -> ScreenModel.Row {
        var r = ScreenModel.Row(id: id, author: author, body: .text(id), sentAt: ms, delivery: author == .me ? .sent : nil)
        r.lineID = line
        return r
    }

    @Test func aDayThatComesBackIsAMarkerOfItsOwnAndEveryIdIsUnique() throws {
        let today = Int64(Self.now.timeIntervalSince1970 * 1000)
        // The Mac's rows in cursor order, today's before yesterday's (its cursors began again),
        // then the phone's own bubble from today.
        let rows = [Self.row("m1", .me, at: today - 60_000), Self.row("r1", .rich, at: today - 50_000),
                    Self.row("old1", .rich, at: today - Self.dayMs), Self.row("old2", .me, at: today - Self.dayMs + 1000),
                    Self.row("local", .me, at: today - 1000)]
        let items = TranscriptItem.build(ScreenModel.Transcript(rows: rows, reachedBeginning: true),
                                         calendar: Self.calendar, now: Self.now)
        let ids = items.map(\.id)
        #expect(ids == ["~beginning", "~day-Today", "m1", "r1", "~day-Yesterday", "old1", "old2", "~day-Today#2", "local"])
        try Self.expectASnapshotTakes(ids)
    }

    @Test func aRowWhoseIdIsAlreadyDrawnIsNotDrawnTwice() throws {
        let today = Int64(Self.now.timeIntervalSince1970 * 1000)
        // Two messages that are one line (the phone's bubble and the Mac's row for it, both carrying
        // the phone's client id): one row.
        let rows = [Self.row("a", .me, at: today - 3000, line: "client-1"), Self.row("b", .rich, at: today - 2000),
                    Self.row("c", .me, at: today - 1000, line: "client-1")]
        let items = TranscriptItem.build(ScreenModel.Transcript(rows: rows), calendar: Self.calendar, now: Self.now)
        let ids = items.map(\.id)
        #expect(ids == ["~day-Today", "client-1", "b"])
        try Self.expectASnapshotTakes(ids)
    }

    /// The call that threw on the phone, made only once the ids are known to be unique (a repeated
    /// id would end this test process the way it ended the app).
    static func expectASnapshotTakes(_ ids: [String]) throws {
        try #require(Set(ids).count == ids.count, "repeated ids: \(ids)")
        var snapshot = NSDiffableDataSourceSnapshot<Int, String>()
        snapshot.appendSections([0])
        snapshot.appendItems(ids, toSection: 0)
        #expect(snapshot.numberOfItems == ids.count)
    }
}
