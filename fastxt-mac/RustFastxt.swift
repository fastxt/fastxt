//
//  RustFastxt.swift
//  Fastxt
//
//  Created by Yi Wang on 2/19/20.
//  Copyright © 2020 Yi Wang. All rights reserved.
//

class RustFastxt {
    func run(json_input: String) -> String {
        let result = fastxt_run(json_input)
        let swift_result = String(cString: result!)
        fastxt_free(UnsafeMutablePointer(mutating: result))
        return swift_result
    }
}
