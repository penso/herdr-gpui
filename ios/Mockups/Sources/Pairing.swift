import SwiftUI

// Nearby pairing, two moments each: finding the computer (state 0) and
// comparing the code (state 1). A: radar. B: a proximity card like
// AirPods setup. C: a plain list, then a checklist.

private let code = ["428", "913"]

private struct CodeDigits: View {
    var body: some View {
        HStack(spacing: 14) {
            ForEach(code, id: \.self) { group in
                Text(group).font(.system(size: 54, weight: .semibold, design: .rounded).monospacedDigit())
            }
        }
    }
}

private struct ViaBadge: View {
    var bluetooth: Bool

    var body: some View {
        Label(bluetooth ? "Bluetooth" : "Wi-Fi", systemImage: bluetooth ? "dot.radiowaves.left.and.right" : "wifi")
            .font(.caption2.weight(.semibold))
            .padding(.horizontal, 8).padding(.vertical, 3)
            .background(.fill.tertiary, in: .capsule)
    }
}

struct PairRadar: View {
    var state: Int

    var body: some View {
        NavigationStack {
            VStack(spacing: 24) {
                if state == 0 {
                    ZStack {
                        ForEach(1..<4) { ring in
                            Circle().strokeBorder(.tint.opacity(0.5 / Double(ring)), lineWidth: 1.5)
                                .frame(width: CGFloat(ring) * 110, height: CGFloat(ring) * 110)
                        }
                        Image(systemName: "iphone").font(.system(size: 34)).foregroundStyle(.tint)
                        RadarDot(name: "MacBook Pro", symbol: "laptopcomputer", bluetooth: true).offset(x: -70, y: -95)
                        RadarDot(name: "studio", symbol: "macstudio", bluetooth: false).offset(x: 88, y: 60)
                    }
                    .frame(height: 340)
                    Text("Open Remote control > Pair device on your computer, then tap it here.")
                        .font(.subheadline).foregroundStyle(.secondary).multilineTextAlignment(.center)
                        .padding(.horizontal, 32)
                } else {
                    Spacer()
                    Image(systemName: "laptopcomputer").font(.system(size: 56)).foregroundStyle(.tint)
                    Text("MacBook Pro").font(.title2.bold())
                    CodeDigits()
                    Text("Check that your Mac shows the same code, then choose **Allow** there.")
                        .font(.subheadline).foregroundStyle(.secondary).multilineTextAlignment(.center)
                        .padding(.horizontal, 40)
                    HStack(spacing: 6) {
                        ProgressView().controlSize(.small)
                        Text("Waiting for your Mac").font(.footnote).foregroundStyle(.secondary)
                    }
                    Spacer()
                }
                Spacer()
                HStack(spacing: 12) {
                    Button {} label: { Label("Scan QR", systemImage: "qrcode.viewfinder").frame(maxWidth: .infinity) }
                    Button {} label: { Label("Paste link", systemImage: "link").frame(maxWidth: .infinity) }
                }
                .buttonStyle(.glass)
                .controlSize(.large)
                .padding(.horizontal, 20)
            }
            .navigationTitle(state == 0 ? "Add a computer" : "Compare the code")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { Button(role: .cancel) {} }
            }
        }
    }
}

private struct RadarDot: View {
    var name: String
    var symbol: String
    var bluetooth: Bool

    var body: some View {
        VStack(spacing: 6) {
            Image(systemName: symbol).font(.title2).frame(width: 64, height: 64)
                .glassEffect(.regular.interactive(), in: .circle)
            Text(name).font(.caption.weight(.semibold))
            ViaBadge(bluetooth: bluetooth)
        }
    }
}

struct PairCard: View {
    var state: Int

    var body: some View {
        ZStack(alignment: .bottom) {
            NavigationStack {
                ContentUnavailableView("No computers yet", systemImage: "desktopcomputer",
                                       description: Text("Bring your phone near a Mac running Herdr."))
                    .navigationTitle("Herdr")
            }
            .blur(radius: 2)
            Color.black.opacity(0.25).ignoresSafeArea()
            VStack(spacing: 16) {
                HStack {
                    Spacer()
                    Image(systemName: "xmark").font(.body.weight(.bold)).frame(width: 30, height: 30)
                        .background(.fill.tertiary, in: .circle)
                }
                Image(systemName: "laptopcomputer").font(.system(size: 80, weight: .thin)).foregroundStyle(.tint)
                    .padding(.vertical, 6)
                Text(state == 0 ? "MacBook Pro" : "Is this your code?").font(.title2.bold())
                if state == 0 {
                    Text("Herdr is asking to pair. Your chats, terminals, and notifications will come to this iPhone.")
                        .font(.subheadline).foregroundStyle(.secondary).multilineTextAlignment(.center)
                    ViaBadge(bluetooth: true)
                    Button {} label: { Text("Pair").font(.headline).frame(maxWidth: .infinity) }
                        .buttonStyle(.glassProminent).controlSize(.extraLarge)
                    Button("Not now") {}.font(.subheadline)
                } else {
                    CodeDigits()
                    Text("MacBook Pro shows a code too. Choose **Allow** there if they match.")
                        .font(.subheadline).foregroundStyle(.secondary).multilineTextAlignment(.center)
                    Button {} label: { Text("They don't match").frame(maxWidth: .infinity) }
                        .buttonStyle(.glass).controlSize(.large).tint(.red)
                }
            }
            .padding(24)
            .background(Color(.secondarySystemBackground), in: .rect(cornerRadius: 44))
            .padding(10)
        }
    }
}

struct PairSteps: View {
    var state: Int

    var body: some View {
        NavigationStack {
            if state == 0 {
                List {
                    Section {
                        NearbyRow(name: "MacBook Pro", detail: "Fabien's MacBook Pro · Herdr 0.42", symbol: "laptopcomputer", bluetooth: true, bars: 3)
                        NearbyRow(name: "studio", detail: "Mac Studio · Herdr 0.42", symbol: "macstudio", bluetooth: false, bars: 2)
                        HStack(spacing: 8) {
                            ProgressView().controlSize(.small)
                            Text("Looking for more").font(.footnote).foregroundStyle(.secondary)
                        }
                    } header: {
                        Text("Nearby")
                    } footer: {
                        Text("A computer appears here while Remote control > Pair device is open on it.")
                    }
                    Section("Other ways") {
                        Label("Scan the QR code", systemImage: "qrcode.viewfinder")
                        Label("Paste a pairing link", systemImage: "link")
                    }
                }
                .navigationTitle("Add a computer")
            } else {
                VStack(alignment: .leading, spacing: 28) {
                    CheckStep(number: 1, title: "Found MacBook Pro", detail: "Over Bluetooth", state: .done)
                    VStack(alignment: .leading, spacing: 12) {
                        CheckStep(number: 2, title: "Compare the code", detail: "Your Mac shows the same six digits", state: .running)
                        CodeDigits().padding(.leading, 40)
                    }
                    CheckStep(number: 3, title: "Allow on your Mac", detail: "Then your chats appear here", state: .pending)
                    Spacer()
                    Label("Fingerprint 7F3A 91C2 · 04BD E1A8", systemImage: "lock.shield")
                        .font(.caption.monospaced()).foregroundStyle(.secondary)
                }
                .padding(24)
                .navigationTitle("Pairing")
            }
        }
    }
}

private struct NearbyRow: View {
    var name: String
    var detail: String
    var symbol: String
    var bluetooth: Bool
    var bars: Int

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: symbol).font(.title3).frame(width: 36)
            VStack(alignment: .leading, spacing: 2) {
                Text(name).font(.body.weight(.medium))
                Text(detail).font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            ViaBadge(bluetooth: bluetooth)
            Image(systemName: "cellularbars", variableValue: Double(bars) / 4).foregroundStyle(.secondary)
        }
    }
}

private struct CheckStep: View {
    var number: Int
    var title: String
    var detail: String
    var state: StepState

    var body: some View {
        HStack(alignment: .top, spacing: 14) {
            ZStack {
                Circle().fill(state == .pending ? AnyShapeStyle(.quaternary) : AnyShapeStyle(.tint))
                if state == .done {
                    Image(systemName: "checkmark").font(.caption.bold()).foregroundStyle(.white)
                } else {
                    Text("\(number)").font(.caption.bold()).foregroundStyle(state == .pending ? AnyShapeStyle(.secondary) : AnyShapeStyle(.white))
                }
            }
            .frame(width: 26, height: 26)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.headline).foregroundStyle(state == .pending ? .secondary : .primary)
                Text(detail).font(.subheadline).foregroundStyle(.secondary)
            }
        }
    }
}
