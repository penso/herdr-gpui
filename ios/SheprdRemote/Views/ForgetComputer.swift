import SheprdRemoteCore
import SwiftUI
import UIKit

extension View {
    /// Asks before forgetting `computer`, in the same words from every
    /// place that offers it; `done` runs after it is forgotten.
    func forgetConfirmation(_ computer: Binding<Computer?>, done: @escaping () -> Void = {}) -> some View {
        modifier(ForgetConfirmation(computer: computer, done: done))
    }
}

private struct ForgetConfirmation: ViewModifier {
    @Environment(AppModel.self) private var model
    @Binding var computer: Computer?
    var done: () -> Void

    func body(content: Content) -> some View {
        content.confirmationDialog(
            computer.map { "Forget \(model.displayName($0))?" } ?? "",
            isPresented: Binding(get: { computer != nil }, set: { if !$0 { computer = nil } }),
            titleVisibility: .visible,
            presenting: computer
        ) { target in
            Button("Forget \(model.displayName(target))", role: .destructive) {
                model.forget(target)
                done()
            }
            Button("Cancel", role: .cancel) {}
        } message: { target in
            Text(ForgetCopy.consequence(model.displayName(target)))
        }
    }
}

/// What forgetting does, said once.
enum ForgetCopy {
    @MainActor
    static func consequence(_ name: String) -> String {
        "Its chats and cached messages leave this \(UIDevice.current.model). Sheprd keeps running on \(name); pair again any time from its Remote control settings."
    }
}

/// Home's first section: a card for each computer that unpaired this
/// phone, to forget it or pair again. Rows of the list, so any number of
/// them scroll with the chats. The connection toast says the same on
/// other screens.
struct UnpairedCards: View {
    @Environment(AppModel.self) private var model
    @Environment(WindowState.self) private var window: WindowState?
    @State private var forgetting: Computer?

    var body: some View {
        let revoked = model.fleet.machines.filter { $0.status == .revoked }.map(\.computer)
        Section {
            ForEach(revoked) { computer in
                card(computer)
                    .listRowInsets(EdgeInsets(top: 4, leading: 0, bottom: 4, trailing: 0))
                    .listRowBackground(Color.clear)
                    .listRowSeparator(.hidden)
            }
        }
        .forgetConfirmation($forgetting)
    }

    private func card(_ computer: Computer) -> some View {
        let device = UIDevice.current.model
        return VStack(alignment: .leading, spacing: 8) {
            Label("\(model.displayName(computer)) unpaired this \(device)", systemImage: "laptopcomputer.slash")
                .font(.subheadline.weight(.semibold)).foregroundStyle(.red)
            Text("Sheprd there removed this \(device), so its chats stop updating here.")
                .font(.caption).foregroundStyle(.secondary)
            HStack {
                Button("Forget", role: .destructive) { forgetting = computer }
                    .buttonStyle(.glass).tint(.red)
                Button("Pair Again") { window?.pairing = true }
                    .buttonStyle(.glassProminent)
            }
            .controlSize(.small)
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassEffect(.regular, in: .rect(cornerRadius: 20))
    }
}
