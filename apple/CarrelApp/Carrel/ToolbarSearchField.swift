// Search in an archive window's toolbar on the Mac (⌘F), as an ordinary toolbar item holding a
// search field, not SwiftUI's `.searchable`. Its toolbar item can be inserted twice as the window
// opens, which AppKit refuses by crashing (#509); an ordinary item can't be.

#if os(macOS)
import AppKit
import SwiftUI

extension View {
    /// The archive window's search field, before the librarian's buttons, so a narrow window
    /// keeps it out of the toolbar's overflow menu.
    func toolbarSearchField(_ model: ArchiveWindowModel) -> some View {
        toolbar {
            ToolbarItem(placement: .primaryAction) {
                ToolbarSearchField(
                    text: Binding(
                        get: { model.search?.text ?? "" },
                        set: { model.search?.text = $0 }),
                    prompt: "Search, or tag:receipts",
                    focusRequests: model.search?.focusRequests ?? 0
                )
                .frame(minWidth: 140, idealWidth: 220, maxWidth: 260)
                .help("Search the archive (⌘F)")
            }
        }
    }
}

/// A search field that searches as it's typed in, is cleared by its clear button or Escape, and
/// takes the keyboard when `focusRequests` goes up.
struct ToolbarSearchField: NSViewRepresentable {
    @Binding var text: String
    let prompt: String
    let focusRequests: Int

    func makeNSView(context: Context) -> NSSearchField {
        let field = NSSearchField()
        field.placeholderString = prompt
        field.setAccessibilityLabel("Search")
        field.sendsSearchStringImmediately = true
        field.target = context.coordinator
        field.action = #selector(Coordinator.searched(_:))
        return field
    }

    func updateNSView(_ field: NSSearchField, context: Context) {
        context.coordinator.text = $text
        if field.stringValue != text { field.stringValue = text }
        if context.coordinator.focusRequests != focusRequests {
            context.coordinator.focusRequests = focusRequests
            DispatchQueue.main.async { field.window?.makeFirstResponder(field) }
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(text: $text, focusRequests: focusRequests)
    }

    @MainActor
    final class Coordinator: NSObject {
        var text: Binding<String>
        var focusRequests: Int

        init(text: Binding<String>, focusRequests: Int) {
            self.text = text
            self.focusRequests = focusRequests
        }

        @objc func searched(_ field: NSSearchField) {
            text.wrappedValue = field.stringValue
        }
    }
}
#endif
