// Flags as requests to the librarian in the app (#454), on a scripted model written in Swift:
// a flagged item handed to the librarian leads to a question in the queue, answering it leads
// to a fix, and accepting the fix clears the flag. Each test has its own copy of the sample
// archive with a configuration folder of its own.

import CarrelKit
import XCTest

@testable import Carrel

final class FlagLibrarianTests: SampleCopyTestCase {
    @MainActor
    func testAFlagLeadsToAQuestionThenAFixThatClearsIt() async throws {
        try await archive.flag(item: sampleNote, note: "the summary is wrong")
        var flags = try await archive.flagRequests()
        XCTAssertEqual(flags.count, 1)
        XCTAssertEqual(flags[0].state, .waiting)

        let asks = ScriptedLibrarianModel([
            ScriptedLibrarianModel.answer("Question: what is the note about, then?")
        ])
        let asked = try await archive.handFlagToLibrarian(sampleNote, model: asks, memoryGB: 16)
        XCTAssertEqual(asked.state, .asked)
        XCTAssertEqual(asked.asking, "what is the note about, then?")
        XCTAssertTrue(
            asks.asked[0].prompt.contains("“the summary is wrong”"), asks.asked[0].prompt)
        let question = try XCTUnwrap(archive.questions.first { $0.kind == .flag })
        XCTAssertEqual(question.id, asked.questionId)
        XCTAssertEqual(question.kindName, "Flag")

        let fixes = ScriptedLibrarianModel([
            ScriptedLibrarianModel.answer(
                "The summary should say it's about the lamp's springs.",
                suggestions: [
                    [
                        "change": "correct", "item": sampleNote, "field": "summary",
                        "value": "Notes on the lamp's coupled springs.",
                        "why": "The owner says so.", "confidence": 0.9,
                    ]
                ])
        ])
        let fixed = try await archive.answerFlagQuestion(
            question.id, answer: "The lamp's springs", model: fixes, memoryGB: 16)
        XCTAssertEqual(fixed.state, .suggested)
        XCTAssertEqual(fixed.fixes.first?.number, 1)
        XCTAssertFalse(archive.questions.contains { $0.kind == .flag }, "answered")

        let accepted = try await archive.acceptFlagFix(sampleNote, number: 1)
        XCTAssertTrue(accepted.cleared)
        flags = try await archive.flagRequests()
        XCTAssertTrue(flags.isEmpty)
        let flag = try await archive.flag(of: sampleNote)
        XCTAssertNil(flag)
    }
}
