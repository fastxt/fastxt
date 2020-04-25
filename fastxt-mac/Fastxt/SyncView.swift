//
//  SyncView.swift
//  Fastxt
//
//  Created by Yi Wang on 4/25/20.
//  Copyright © 2020 Yi Wang. All rights reserved.
//

import SwiftUI

struct SyncView: View {
    var body: some View {
        VStack{
            ServerView()
            ClientView()
        }
    }
}

struct SyncView_Previews: PreviewProvider {
    static var previews: some View {
        SyncView()
    }
}
