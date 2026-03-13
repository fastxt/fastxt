package app.fastxt.android

import android.content.Intent
import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity
import androidx.recyclerview.widget.RecyclerView
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import org.json.JSONObject

import app.fastxt.android.dummy.DummyContent
import kotlinx.android.synthetic.main.activity_item_list.*
import kotlinx.android.synthetic.main.item_list_content.view.*
import kotlinx.android.synthetic.main.item_list.*

/**
 * An activity representing a list of Notes. This activity
 * has different presentations for handset and tablet-size devices. On
 * handsets, the activity presents a list of items, which when touched,
 * lead to a [ItemDetailActivity] representing
 * item details. On tablets, the activity presents the list of items and
 * item details side-by-side using two vertical panes.
 */
class ItemListActivity : AppCompatActivity() {

    /**
     * Whether or not the activity is in two-pane mode, i.e. running on a tablet
     * device.
     */
    private var twoPane: Boolean = false
    private var notes: MutableList<Note> = mutableListOf()
    private lateinit var adapter: NoteRecyclerViewAdapter

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_item_list)

        setSupportActionBar(toolbar)
        toolbar.title = title

        fab.setOnClickListener {
            CreateNoteDialog(this) {
                refreshNotes()
            }.show()
        }

        if (item_detail_container != null) {
            twoPane = true
        }

        adapter = NoteRecyclerViewAdapter(this, notes, twoPane)
        item_list.adapter = adapter

        refreshNotes()
    }

    /**
     * Refresh the notes list from the database.
     */
    fun refreshNotes() {
        try {
            val response = RustBridge.search("", 0)
            val json = JSONObject(response)
            val notesArray = json.getJSONArray("notes")

            notes.clear()
            for (i in 0 until notesArray.length()) {
                val noteJson = notesArray.getJSONObject(i)
                notes.add(Note.fromJson(noteJson))
            }

            adapter.notifyDataSetChanged()
        } catch (e: Exception) {
            // Fallback to dummy content if database not available
            notes.clear()
            notes.addAll(DummyContent.ITEMS.map {
                Note(it.id.toLongOrNull() ?: 0, "", it.content, "", "")
            })
            adapter.notifyDataSetChanged()
        }
    }

    class NoteRecyclerViewAdapter(
        private val parentActivity: ItemListActivity,
        private val values: List<Note>,
        private val twoPane: Boolean
    ) : RecyclerView.Adapter<NoteRecyclerViewAdapter.ViewHolder>() {

        private val onClickListener: View.OnClickListener

        init {
            onClickListener = View.OnClickListener { v ->
                val note = v.tag as Note
                if (twoPane) {
                    val fragment = ItemDetailFragment().apply {
                        arguments = Bundle().apply {
                            putLong(ItemDetailFragment.ARG_NOTE_ID, note.id)
                            putString(ItemDetailFragment.ARG_NOTE_TEXT, note.txt)
                            putString(ItemDetailFragment.ARG_NOTE_TAGS, note.tags)
                        }
                    }
                    parentActivity.supportFragmentManager
                            .beginTransaction()
                            .replace(R.id.item_detail_container, fragment)
                            .commit()
                } else {
                    val intent = Intent(v.context, ItemDetailActivity::class.java).apply {
                        putExtra(ItemDetailFragment.ARG_NOTE_ID, note.id)
                        putExtra(ItemDetailFragment.ARG_NOTE_TEXT, note.txt)
                        putExtra(ItemDetailFragment.ARG_NOTE_TAGS, note.tags)
                    }
                    v.context.startActivity(intent)
                }
            }
        }

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): ViewHolder {
            val view = LayoutInflater.from(parent.context)
                    .inflate(R.layout.item_list_content, parent, false)
            return ViewHolder(view)
        }

        override fun onBindViewHolder(holder: ViewHolder, position: Int) {
            val item = values[position]
            holder.idView.text = "#${item.id}"
            holder.contentView.text = item.txt.take(100) + if (item.txt.length > 100) "..." else ""

            with(holder.itemView) {
                tag = item
                setOnClickListener(onClickListener)
            }
        }

        override fun getItemCount() = values.size

        inner class ViewHolder(view: View) : RecyclerView.ViewHolder(view) {
            val idView: TextView = view.id_text
            val contentView: TextView = view.content
        }
    }
}
