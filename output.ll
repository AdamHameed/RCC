; ModuleID = 'main'
source_filename = "main"
target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128"
target triple = "arm64-apple-darwin24.6.0"

define i32 @main() {
entry:
  ret i32 12
}
