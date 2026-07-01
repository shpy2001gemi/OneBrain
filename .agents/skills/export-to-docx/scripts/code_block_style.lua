-- Pandoc Lua filter to style code blocks in DOCX output
-- Applies: gray background, border, monospace font, smaller size
-- Usage: pandoc ... --lua-filter=code_block_style.lua

function CodeBlock(block)
  -- Wrap code block in a styled container using OpenXML raw block
  local code_text = block.text
  local lang = block.classes[1] or ""

  -- Build OpenXML paragraphs for code block with styling
  local lines = {}
  for line in (code_text .. "\n"):gmatch("(.-)\n") do
    table.insert(lines, line)
  end

  local xml_parts = {}
  for i, line in ipairs(lines) do
    -- Escape XML special characters
    local escaped = line
      :gsub("&", "&amp;")
      :gsub("<", "&lt;")
      :gsub(">", "&gt;")
      :gsub('"', "&quot;")
    -- Replace spaces with non-breaking to preserve indentation
    escaped = escaped:gsub("  ", "&#160;&#160;")

    -- Determine border settings: 
    -- top border only on first line, bottom only on last, sides on all
    local top_border = ""
    local bottom_border = ""
    local between_border = ""

    if i == 1 then
      top_border = '<w:top w:val="single" w:sz="4" w:space="4" w:color="CCCCCC"/>'
    else
      top_border = '<w:top w:val="nil"/>'
    end

    if i == #lines then
      bottom_border = '<w:bottom w:val="single" w:sz="4" w:space="4" w:color="CCCCCC"/>'
    else
      bottom_border = '<w:bottom w:val="nil"/>'
    end

    local para_xml = string.format([[
<w:p>
  <w:pPr>
    <w:pStyle w:val="SourceCode"/>
    <w:pBdr>
      %s
      %s
      <w:left w:val="single" w:sz="4" w:space="6" w:color="CCCCCC"/>
      <w:right w:val="single" w:sz="4" w:space="6" w:color="CCCCCC"/>
    </w:pBdr>
    <w:shd w:val="clear" w:color="auto" w:fill="F5F5F5"/>
    <w:spacing w:before="0" w:after="0" w:line="276" w:lineRule="auto"/>
    <w:ind w:left="120" w:right="120"/>
  </w:pPr>
  <w:r>
    <w:rPr>
      <w:rFonts w:ascii="Consolas" w:hAnsi="Consolas" w:cs="Consolas"/>
      <w:sz w:val="18"/>
      <w:szCs w:val="18"/>
      <w:color w:val="1F2937"/>
    </w:rPr>
    <w:t xml:space="preserve">%s</w:t>
  </w:r>
</w:p>]], top_border, bottom_border, escaped)

    table.insert(xml_parts, para_xml)
  end

  -- Language label as first line if present
  local lang_label = ""
  if lang ~= "" then
    lang_label = string.format([[
<w:p>
  <w:pPr>
    <w:pBdr>
      <w:top w:val="single" w:sz="4" w:space="4" w:color="CCCCCC"/>
      <w:left w:val="single" w:sz="4" w:space="4" w:color="CCCCCC"/>
      <w:right w:val="single" w:sz="4" w:space="4" w:color="CCCCCC"/>
      <w:bottom w:val="nil"/>
    </w:pBdr>
    <w:shd w:val="clear" w:color="auto" w:fill="E8E8E8"/>
    <w:spacing w:before="120" w:after="0"/>
    <w:ind w:left="120" w:right="120"/>
  </w:pPr>
  <w:r>
    <w:rPr>
      <w:rFonts w:ascii="Consolas" w:hAnsi="Consolas" w:cs="Consolas"/>
      <w:sz w:val="16"/>
      <w:szCs w:val="16"/>
      <w:color w:val="6B7280"/>
      <w:b/>
    </w:rPr>
    <w:t>%s</w:t>
  </w:r>
</w:p>]], lang)
    -- Remove top border from first code line since lang label has it
    if #xml_parts > 0 then
      xml_parts[1] = xml_parts[1]:gsub(
        '<w:top w:val="single" w:sz="4" w:space="4" w:color="CCCCCC"/>',
        '<w:top w:val="nil"/>'
      )
    end
  end

  local full_xml = lang_label .. table.concat(xml_parts, "\n")
  return pandoc.RawBlock("openxml", full_xml)
end
