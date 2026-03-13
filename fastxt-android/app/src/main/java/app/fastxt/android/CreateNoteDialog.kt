/*
    Fastxt
    Copyright (C) 2020  Yi Wang

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU Affero General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU Affero General Public License for more details.

    You should have received a copy of the GNU Affero General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

package app.fastxt.android

import android.app.AlertDialog
import android.content.Context
import android.text.Editable
import android.text.TextWatcher
import android.view.LayoutInflater
import android.view.View
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.Toast
import app.fastxt.android.ai.AiTagsResponse
import app.fastxt.android.ai.FastxtAI
import com.google.android.material.chip.Chip
import com.google.android.material.chip.ChipGroup
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Dialog for creating a new note with AI tag suggestions.
 */
class CreateNoteDialog(
    private val context: Context,
    private val onNoteCreated: () -> Unit
) {
    private var dialog: AlertDialog? = null
    private var suggestedTags: List<String> = emptyList()
    private var isAiAvailable = false

    fun show() {
        val view = LayoutInflater.from(context).inflate(R.layout.dialog_create_note, null)

        val txtInput = view.findViewById<EditText>(R.id.txt_input)
        val tagsInput = view.findViewById<EditText>(R.id.tags_input)
        val aiTagsContainer = view.findViewById<LinearLayout>(R.id.ai_tags_container)
        val aiTagsChipGroup = view.findViewById<ChipGroup>(R.id.ai_tags_chip_group)
        val aiStatusText = view.findViewById<TextView>(R.id.ai_status_text)
        val btnGetAiTags = view.findViewById<Button>(R.id.btn_get_ai_tags)
        val btnUseAllTags = view.findViewById<Button>(R.id.btn_use_all_tags)

        // Check AI availability
        isAiAvailable = FastxtAI.isAvailable()
        if (!isAiAvailable) {
            btnGetAiTags.visibility = View.GONE
            aiStatusText.text = "AI features require Android 14+"
            aiStatusText.visibility = View.VISIBLE
        }

        // Text watcher to enable/disable AI button
        txtInput.addTextChangedListener(object : TextWatcher {
            override fun afterTextChanged(s: Editable?) {
                btnGetAiTags.isEnabled = !s.isNullOrBlank()
            }
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
        })

        // Get AI tags button
        btnGetAiTags.setOnClickListener {
            val text = txtInput.text.toString()
            if (text.isNotBlank()) {
                aiStatusText.text = "Getting AI suggestions..."
                aiStatusText.visibility = View.VISIBLE

                CoroutineScope(Dispatchers.Main).launch {
                    val result = withContext(Dispatchers.IO) {
                        FastxtAI.suggestTags(text)
                    }

                    val response = AiTagsResponse.fromJson(result)
                    if (response != null && response.tags.isNotEmpty()) {
                        suggestedTags = response.tags
                        showAiTags(aiTagsContainer, aiTagsChipGroup, tagsInput)
                        aiStatusText.visibility = View.GONE
                    } else {
                        aiStatusText.text = response?.error ?: "No tags suggested"
                        aiStatusText.visibility = View.VISIBLE
                    }
                }
            }
        }

        // Use all tags button
        btnUseAllTags.setOnClickListener {
            if (suggestedTags.isNotEmpty()) {
                val currentTags = tagsInput.text.toString()
                val newTags = if (currentTags.isNotBlank()) {
                    "$currentTags,${suggestedTags.joinToString(",")}"
                } else {
                    suggestedTags.joinToString(",")
                }
                tagsInput.setText(newTags)
                aiTagsContainer.visibility = View.GONE
                suggestedTags = emptyList()
            }
        }

        dialog = AlertDialog.Builder(context)
            .setTitle("New Note")
            .setView(view)
            .setPositiveButton("Save") { _, _ ->
                val txt = txtInput.text.toString()
                val tags = tagsInput.text.toString()

                if (txt.isNotBlank()) {
                    val success = RustBridge.insert(txt, tags)
                    if (success) {
                        Toast.makeText(context, "Note saved", Toast.LENGTH_SHORT).show()
                        onNoteCreated()
                    } else {
                        Toast.makeText(context, "Failed to save note", Toast.LENGTH_SHORT).show()
                    }
                }
            }
            .setNegativeButton("Cancel", null)
            .create()

        dialog?.show()
    }

    private fun showAiTags(
        container: LinearLayout,
        chipGroup: ChipGroup,
        tagsInput: EditText
    ) {
        chipGroup.removeAllViews()

        for (tag in suggestedTags) {
            val chip = Chip(context).apply {
                text = tag
                isClickable = true
                isCheckable = false
                setOnClickListener {
                    val currentTags = tagsInput.text.toString()
                    val newTags = if (currentTags.isNotBlank()) {
                        "$currentTags,$tag"
                    } else {
                        tag
                    }
                    tagsInput.setText(newTags)
                }
            }
            chipGroup.addView(chip)
        }

        container.visibility = View.VISIBLE
    }
}
