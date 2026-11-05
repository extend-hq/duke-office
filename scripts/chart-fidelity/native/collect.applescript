on run argv
  set wbName to item 1 of argv
  with timeout of 30 seconds
    tell application "Microsoft Excel"
      if (name of active workbook) is not wbName then error "Active workbook changed"
      if (item 2 of argv) is "inventory" then
        set recordLines to {}
        repeat with s from 1 to (count worksheets of active workbook)
          repeat with k from 1 to (count chart objects of worksheet s of active workbook)
            set co to chart object k of worksheet s of active workbook
            set end of recordLines to "object" & (character id 9) & name of worksheet s of active workbook & (character id 9) & (s as string) & "," & (k as string) & (character id 9) & name of co & (character id 9) & (width of co as string) & (character id 9) & (height of co as string)
          end repeat
        end repeat
        try
          repeat with cs in (get chart sheets of active workbook)
            set end of recordLines to "sheet" & (character id 9) & name of cs & (character id 9) & "1" & (character id 9) & name of cs & (character id 9) & "0" & (character id 9) & "0"
          end repeat
        end try
        set AppleScript's text item delimiters to linefeed
        return recordLines as string
      else if (item 2 of argv) is "close" then
        close active workbook saving no
        repeat with openBook in workbooks
          if (name of openBook) is wbName then error "Temporary capture workbook is still open; close it without saving before continuing"
        end repeat
      else if (item 2 of argv) is "sheet" then
        set cs to chart sheet (item 3 of argv) of active workbook
        select cs
        copy chart as picture (chart of cs)
      else
        set AppleScript's text item delimiters to ","
        set slots to text items of (item 4 of argv)
        select worksheet ((item 1 of slots) as integer) of active workbook
        set co to chart object ((item 2 of slots) as integer) of worksheet ((item 1 of slots) as integer) of active workbook
        «event sCRT1539» co
      end if
    end tell
  end timeout
end run
