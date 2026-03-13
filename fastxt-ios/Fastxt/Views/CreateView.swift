//
//  CreateView.swift
//  Fastxt
//
//  Created by Yi Wang on 4/23/20.
//  Copyright © 2020 Yi Wang. All rights reserved.
//

import SwiftUI

struct CreateView: View {
    @Binding var isPresented: Bool
    @State var tags: String = ""
    @State var txt: String = ""
    @State var textHeight: CGFloat = 150
    @ObservedObject var env = AppState.getEnv()

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {

            HStack{
                Button(action:{
                    self.tags = ""
                    self.txt = ""
                    env.clearAiTags()
                }){
                    Text("Reset")
                }
                Spacer()
                // AI Tags button
                Button(action:{
                    env.getAiTags(for: self.txt)
                }){
                    HStack(spacing: 4) {
                        Image(systemName: "brain")
                        Text("AI Tags")
                    }
                }
                .disabled(txt.isEmpty)
                Spacer()
                Button(action:{
                    AppState.insert(txt: self.txt, tags: self.tags)
                    self.isPresented = false
                    AppState.search(input: "", offset: 0)
                }){
                    Text("Save")
                }
            }

            // AI Suggested Tags Section
            if env.showAiTags && !env.aiSuggestedTags.isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        Text("AI Suggested Tags:")
                            .font(.caption)
                            .foregroundColor(.gray)
                        Spacer()
                        Button("Dismiss") {
                            env.clearAiTags()
                        }
                        .font(.caption)
                    }

                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 8) {
                            ForEach(env.aiSuggestedTags, id: \.self) { tag in
                                Button(action: {
                                    if self.tags.isEmpty {
                                        self.tags = tag
                                    } else {
                                        self.tags = "\(self.tags),\(tag)"
                                    }
                                }) {
                                    Text(tag)
                                        .padding(.horizontal, 12)
                                        .padding(.vertical, 6)
                                        .background(Color.blue.opacity(0.1))
                                        .foregroundColor(.blue)
                                        .cornerRadius(12)
                                }
                            }
                            Button("Use All") {
                                self.tags = env.useAiTags(currentTags: self.tags)
                            }
                            .padding(.horizontal, 12)
                            .padding(.vertical, 6)
                            .background(Color.green.opacity(0.1))
                            .foregroundColor(.green)
                            .cornerRadius(12)
                        }
                    }
                }
                .padding(8)
                .background(Color.gray.opacity(0.05))
                .cornerRadius(8)
            }

            // AI Status
            if !env.aiStatus.isEmpty {
                Text(env.aiStatus)
                    .font(.caption)
                    .foregroundColor(.orange)
            }

            TextField("enter tags: comma or space as tag seperator", text: $tags)
            Text("enter txt below:").foregroundColor(.gray)
            TextView(placeholder: "write your txt here ...", text: self.$txt, minHeight: self.textHeight, calculatedHeight: self.$textHeight)
                .frame(minHeight: self.textHeight, maxHeight: self.textHeight)

            Spacer()
        }.padding()

    }
}

struct CreateView_Previews: PreviewProvider {
    static var previews: some View {
        CreateView(isPresented: .constant(true))
    }
}

struct TextView: UIViewRepresentable {
    var placeholder: String
    @Binding var text: String

    var minHeight: CGFloat
    @Binding var calculatedHeight: CGFloat

    init(placeholder: String, text: Binding<String>, minHeight: CGFloat, calculatedHeight: Binding<CGFloat>) {
        self.placeholder = placeholder
        self._text = text
        self.minHeight = minHeight
        self._calculatedHeight = calculatedHeight
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    func makeUIView(context: Context) -> UITextView {
        let textView = UITextView()
        textView.delegate = context.coordinator

        // Decrease priority of content resistance, so content would not push external layout set in SwiftUI
        textView.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        textView.isScrollEnabled = false
        textView.isEditable = true
        textView.isUserInteractionEnabled = true
        textView.isSelectable = true
        //textView.backgroundColor = UIColor(white: 0.0, alpha: 0.05)

        // Set the placeholder
        textView.text = placeholder
        //textView.textColor = UIColor.lightGray

        return textView
    }

    func updateUIView(_ textView: UITextView, context: Context) {
        if textView.text != self.text {
            textView.text = self.text
        }

        recalculateHeight(view: textView)
    }

    func recalculateHeight(view: UIView) {
        let newSize = view.sizeThatFits(CGSize(width: view.frame.size.width, height: CGFloat.greatestFiniteMagnitude))
        if minHeight < newSize.height && $calculatedHeight.wrappedValue != newSize.height {
            DispatchQueue.main.async {
                self.$calculatedHeight.wrappedValue = newSize.height // !! must be called asynchronously
            }
        } else if minHeight >= newSize.height && $calculatedHeight.wrappedValue != minHeight {
            DispatchQueue.main.async {
                self.$calculatedHeight.wrappedValue = self.minHeight // !! must be called asynchronously
            }
        }
    }

    class Coordinator : NSObject, UITextViewDelegate {

        var parent: TextView

        init(_ uiTextView: TextView) {
            self.parent = uiTextView
        }

        func textViewDidChange(_ textView: UITextView) {
            // This is needed for multistage text input (eg. Chinese, Japanese)
            if textView.markedTextRange == nil {
                parent.text = textView.text ?? String()
                parent.recalculateHeight(view: textView)
            }
        }

        func textViewDidBeginEditing(_ textView: UITextView) {
            if textView.textColor == UIColor.lightGray {
                textView.text = nil
                textView.textColor = UIColor.black
            }
        }

        func textViewDidEndEditing(_ textView: UITextView) {
            if textView.text.isEmpty {
                textView.text = parent.placeholder
                textView.textColor = UIColor.lightGray
            }
        }
    }
}
