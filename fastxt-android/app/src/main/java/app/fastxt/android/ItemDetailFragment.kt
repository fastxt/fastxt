package app.fastxt.android

import android.os.Bundle
import androidx.fragment.app.Fragment
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.TextView
import android.widget.Toast
import app.fastxt.android.ai.AiSummaryResponse
import app.fastxt.android.ai.FastxtAI
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * A fragment representing a single Note detail screen.
 * This fragment is either contained in a [ItemListActivity]
 * in two-pane mode (on tablets) or a [ItemDetailActivity]
 * on handsets.
 */
class ItemDetailFragment : Fragment() {

    private var noteId: Long = 0
    private var noteText: String = ""
    private var noteTags: String = ""
    private var aiSummary: String? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        arguments?.let {
            noteId = it.getLong(ARG_NOTE_ID, 0)
            noteText = it.getString(ARG_NOTE_TEXT, "")
            noteTags = it.getString(ARG_NOTE_TAGS, "")
        }
    }

    override fun onCreateView(inflater: LayoutInflater, container: ViewGroup?,
                              savedInstanceState: Bundle?): View? {
        val rootView = inflater.inflate(R.layout.item_detail, container, false)

        // Note text
        val noteTextView = rootView.findViewById<TextView>(R.id.item_detail)
        noteTextView.text = noteText

        // Tags display
        val tagsTextView = rootView.findViewById<TextView>(R.id.tags_text)
        if (noteTags.isNotBlank()) {
            tagsTextView.visibility = View.VISIBLE
            tagsTextView.text = "Tags: $noteTags"
        } else {
            tagsTextView.visibility = View.GONE
        }

        // AI Summary section
        val summaryContainer = rootView.findViewById<ViewGroup>(R.id.summary_container)
        val summaryTextView = rootView.findViewById<TextView>(R.id.summary_text)
        val summarizeButton = rootView.findViewById<Button>(R.id.btn_summarize)
        val aiStatusText = rootView.findViewById<TextView>(R.id.ai_status_text)

        // Check AI availability
        val isAiAvailable = FastxtAI.isAvailable()
        if (!isAiAvailable) {
            summarizeButton.visibility = View.GONE
            aiStatusText.text = "AI features require Android 14+"
            aiStatusText.visibility = View.VISIBLE
        }

        summarizeButton.setOnClickListener {
            aiStatusText.text = "Generating summary..."
            aiStatusText.visibility = View.VISIBLE

            CoroutineScope(Dispatchers.Main).launch {
                val result = withContext(Dispatchers.IO) {
                    FastxtAI.summarize(noteText)
                }

                val response = AiSummaryResponse.fromJson(result)
                if (response != null && response.summary != null) {
                    aiSummary = response.summary
                    summaryTextView.text = aiSummary
                    summaryContainer.visibility = View.VISIBLE
                    aiStatusText.visibility = View.GONE
                } else {
                    aiStatusText.text = response?.error ?: "Failed to generate summary"
                }
            }
        }

        activity?.title = "Note #$noteId"

        return rootView
    }

    companion object {
        const val ARG_ITEM_ID = "item_id"
        const val ARG_NOTE_ID = "note_id"
        const val ARG_NOTE_TEXT = "note_text"
        const val ARG_NOTE_TAGS = "note_tags"
    }
}
