//
//  ServerView.swift
//  Fastxt
//
//  Created by Yi Wang on 4/24/20.
//  Copyright © 2020 Yi Wang. All rights reserved.
//

import SwiftUI
import CoreImage.CIFilterBuiltins

struct ServerView: View {
    @EnvironmentObject var env : Env
    var body: some View {  
        VStack(alignment: .leading){
            Text("As Server")
            HStack{
                if env.isServerRunning{
                    Button(action:{
                        DispatchQueue.background(background: {
                            AppState.ft.run(json_input: """
                                {"action":"client-stop-server", "addr":"\(self.env.addr)"}
                            """)
                        }, completion:{
                            
                            print("done client stop server")
                        })

                    }){
                        Text("Stop Server")
                    }

                }else{
                    Button(action:{
                        self.env.addr = AppState.ft.run(json_input: """
                            {"action":"server-addr"}
                        """)
                        self.env.isServerRunning = true
                        DispatchQueue.background(background: {
                            AppState.ft.run(json_input: """
                                {"action":"server", "addr":"\(self.env.addr)"}
                            """)
                        }, completion:{
                            self.env.isServerRunning = false
                            print("done server")
                        })
                    }){
                        Text("Start Server")
                    }
                }


                Spacer()
            }
            
            if env.isServerRunning{
                Text("Server address:port")
                Text(env.addr)
                QRCodeView().environmentObject(AppState.env)
            }
            Spacer()
        }
    }
}

struct ServerView_Previews: PreviewProvider {
    static var previews: some View {
        ServerView()
    }
}

