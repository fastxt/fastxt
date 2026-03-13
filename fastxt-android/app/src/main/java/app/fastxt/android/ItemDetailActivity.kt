package app.fastxt.android

import android.content.Intent
import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity
import androidx.core.app.NavUtils
import android.view.MenuItem
import kotlinx.android.synthetic.main.activity_item_detail.*

/**
 * An activity representing a single Note detail screen. This
 * activity is only used on narrow width devices. On tablet-size devices,
 * item details are presented side-by-side with a list of items
 * in a [ItemListActivity].
 */
class ItemDetailActivity : AppCompatActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_item_detail)
        setSupportActionBar(detail_toolbar)

        // Show the Up button in the action bar.
        supportActionBar?.setDisplayHomeAsUpEnabled(true)

        if (savedInstanceState == null) {
            val fragment = ItemDetailFragment().apply {
                arguments = Bundle().apply {
                    putLong(ItemDetailFragment.ARG_NOTE_ID,
                            intent.getLongExtra(ItemDetailFragment.ARG_NOTE_ID, 0))
                    putString(ItemDetailFragment.ARG_NOTE_TEXT,
                            intent.getStringExtra(ItemDetailFragment.ARG_NOTE_TEXT))
                    putString(ItemDetailFragment.ARG_NOTE_TAGS,
                            intent.getStringExtra(ItemDetailFragment.ARG_NOTE_TAGS))
                }
            }

            supportFragmentManager.beginTransaction()
                    .add(R.id.item_detail_container, fragment)
                    .commit()
        }
    }

    override fun onOptionsItemSelected(item: MenuItem) =
            when (item.itemId) {
                android.R.id.home -> {
                    NavUtils.navigateUpTo(this, Intent(this, ItemListActivity::class.java))
                    true
                }
                else -> super.onOptionsItemSelected(item)
            }
}
